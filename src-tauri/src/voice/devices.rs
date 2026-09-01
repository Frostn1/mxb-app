//! Microphone and speaker plumbing for in-game voice.
//!
//! This is the device half of voice chat: which microphone we listen to, which output the
//! other riders come out of, how loud, and the key that opens the mic. What it gives the
//! player is the part they have to get right *before* any of the rest can help them: proof
//! that the app can hear them, and proof they'll hear everyone else.
//!
//! Two deliberate choices:
//!
//! **A blank device name means "follow the system default".** Storing the resolved name
//! instead would pin the player to whichever headset happened to be plugged in the day
//! they opened this page, and stop tracking the default they change in Windows later.
//!
//! **A missing device falls back to the default and says so.** Headsets get unplugged
//! mid-session. Silently going mute is the failure that reads as "voice chat is broken",
//! so a device we can't find is reported, not swallowed.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};

/// How often the input meter reports a level to the UI. Fast enough to feel live when you
/// tap the mic, slow enough that it isn't a stream of events behind a settings page.
const METER_HZ: u64 = 20;

/// Length of the output test tone. Long enough to identify which headphone it came out of,
/// short enough that nobody needs a stop button.
const TEST_TONE_MS: u64 = 600;

/// A polite 440 Hz at low amplitude — this plays into a headset someone is wearing.
const TEST_TONE_ADSR_PEAK: f32 = 0.18;

/// One selectable audio device.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub name: String,
    /// This is what the OS would pick. The UI marks it, and it's what a blank setting means.
    pub is_default: bool,
}

/// Everything the settings page needs to draw its two pickers.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Devices {
    pub inputs: Vec<DeviceInfo>,
    pub outputs: Vec<DeviceInfo>,
    /// Set when the host has no audio at all — a machine with no sound card, or a
    /// permissions refusal. The UI shows this instead of two empty dropdowns.
    pub error: Option<String>,
}

/// Live meter level, emitted to the webview as `voice-input-level`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    /// 0..1, already gain-adjusted — what the app would actually transmit.
    pub rms: f32,
    /// 0..1 peak since the last report; drives the clip indicator.
    pub peak: f32,
}

/// Push-to-talk state, shared with whatever eventually does the transmitting.
static PTT_DOWN: AtomicBool = AtomicBool::new(false);

/// Whether the key itself is physically down. Only toggle mode needs this, to tell a fresh
/// press from the auto-repeat the OS sends while a key is held — in push-to-talk the mic
/// state *is* the key state, so the distinction doesn't arise.
static KEY_HELD: AtomicBool = AtomicBool::new(false);

/// Whether the mic is currently open — held in push-to-talk, latched in toggle.
pub fn transmitting() -> bool {
    PTT_DOWN.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------------------
// Enumeration
// ---------------------------------------------------------------------------------------

/// List every input and output the host offers.
///
/// Enumerated fresh on every call rather than cached at startup: the whole point is to
/// notice the headset that was plugged in after the app launched.
pub fn devices() -> Devices {
    let host = cpal::default_host();
    let default_in = host.default_input_device().and_then(|d| device_name(&d));
    let default_out = host.default_output_device().and_then(|d| device_name(&d));

    let inputs = collect(host.input_devices().ok(), default_in.as_deref());
    let outputs = collect(host.output_devices().ok(), default_out.as_deref());
    let error = (inputs.is_empty() && outputs.is_empty())
        .then(|| "No audio devices were found on this machine.".to_string());

    Devices { inputs, outputs, error }
}

/// The human-readable name, or `None` for a device that has gone away mid-enumeration.
///
/// A device unplugged between being listed and being named fails here, and is skipped
/// rather than shown as a row that can't be selected.
fn device_name(device: &cpal::Device) -> Option<String> {
    device.name().ok()
}

fn collect<I: Iterator<Item = cpal::Device>>(list: Option<I>, default: Option<&str>) -> Vec<DeviceInfo> {
    let Some(list) = list else { return Vec::new() };
    let mut out: Vec<DeviceInfo> = Vec::new();
    for device in list {
        let Some(name) = device_name(&device) else { continue };
        // A host can report the same endpoint twice; a duplicate row is confusing to pick
        // from and means nothing different when selected.
        if out.iter().any(|d| d.name == name) {
            continue;
        }
        let is_default = default == Some(name.as_str());
        out.push(DeviceInfo { name, is_default });
    }
    out
}

/// Resolve a saved setting to a real device.
///
/// `wanted` blank → the system default, which is the setting's whole meaning. A name we
/// can't find → the default too, but the caller is told, because a player who picked a
/// specific headset needs to know they're not on it.
pub(super) fn resolve(wanted: &str, input: bool) -> Result<(cpal::Device, Option<String>), String> {
    let host = cpal::default_host();
    let default = || {
        if input { host.default_input_device() } else { host.default_output_device() }
    };

    let wanted = wanted.trim();
    if !wanted.is_empty() {
        let listed = if input { host.input_devices() } else { host.output_devices() };
        if let Ok(mut list) = listed {
            if let Some(found) = list.find(|d| device_name(d).as_deref() == Some(wanted)) {
                return Ok((found, None));
            }
        }
        let device = default().ok_or_else(|| no_device_msg(input))?;
        return Ok((
            device,
            Some(format!("\"{wanted}\" isn't connected — using the system default instead.")),
        ));
    }

    Ok((default().ok_or_else(|| no_device_msg(input))?, None))
}

fn no_device_msg(input: bool) -> String {
    if input {
        "No microphone is available.".to_string()
    } else {
        "No speakers or headphones are available.".to_string()
    }
}

// ---------------------------------------------------------------------------------------
// Input meter
// ---------------------------------------------------------------------------------------

/// Owns the running mic-test thread, if there is one. Held in Tauri state.
#[derive(Default)]
pub struct Monitor {
    stop: Mutex<Option<mpsc::Sender<()>>>,
}

impl Monitor {
    /// Open the microphone and report its level until [`Monitor::stop`] is called.
    ///
    /// The stream lives on its own thread because `cpal::Stream` is not `Send` — it cannot
    /// be parked in shared state and dropped from another thread later. The thread owns it,
    /// and the channel is how we ask that thread to let go.
    pub fn start<R: Runtime>(&self, app: AppHandle<R>, device: &str, gain: f32) -> Result<Option<String>, String> {
        self.stop();

        let (tx, rx) = mpsc::channel::<()>();
        // Built here rather than on the thread so a bad device reports an error to the
        // caller, instead of the UI starting a meter that silently never ticks.
        let (device, warning) = resolve(device, true)?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("Couldn't read the microphone's format: {e}"))?;

        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        std::thread::spawn(move || {
            match run_meter(app, device, config, gain, rx, &ready_tx) {
                Ok(()) => {}
                Err(e) => {
                    // If it failed before signalling ready, `start` is still waiting.
                    let _ = ready_tx.send(Err(e));
                }
            }
        });

        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {
                *self.stop.lock().unwrap() = Some(tx);
                Ok(warning)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err("The microphone didn't start in time.".to_string()),
        }
    }

    /// Close the microphone. Idempotent — stopping a meter that isn't running is fine, and
    /// is the common case when the settings page unmounts.
    pub fn stop(&self) {
        if let Some(tx) = self.stop.lock().unwrap().take() {
            let _ = tx.send(());
        }
    }
}

/// Body of the meter thread: build the stream, poll the levels it writes, emit them.
fn run_meter<R: Runtime>(
    app: AppHandle<R>,
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
    gain: f32,
    stop: mpsc::Receiver<()>,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    // The audio callback runs on a realtime thread: it may not allocate, lock, or emit
    // events. It writes two numbers here and this thread does the talking.
    //
    // The accumulate-then-reset across these atomics is a benign race — a swap landing
    // between the callback's load and store keeps one window's energy for the next one.
    // Worst case a meter bar is briefly high; the alternative is a CAS loop on a
    // realtime thread to make a UI indicator marginally more accurate.
    let sum_sq = Arc::new(AtomicU32::new(0)); // f32 bits
    let peak = Arc::new(AtomicU32::new(0)); // f32 bits
    let samples = Arc::new(AtomicU32::new(0));

    let (cb_sum, cb_peak, cb_n) = (sum_sq.clone(), peak.clone(), samples.clone());
    let gain = gain.clamp(0.0, 4.0);

    let err_fn = |e| log::warn!("voice: input stream error: {e}");
    let stream = device
        .build_input_stream(
            &config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut s = f32::from_bits(cb_sum.load(Ordering::Relaxed));
                let mut p = f32::from_bits(cb_peak.load(Ordering::Relaxed));
                for &sample in data {
                    let v = (sample * gain).clamp(-1.0, 1.0);
                    s += v * v;
                    let a = v.abs();
                    if a > p {
                        p = a;
                    }
                }
                cb_sum.store(s.to_bits(), Ordering::Relaxed);
                cb_peak.store(p.to_bits(), Ordering::Relaxed);
                cb_n.fetch_add(data.len() as u32, Ordering::Relaxed);
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("Couldn't open the microphone: {e}"))?;

    stream.play().map_err(|e| format!("Couldn't start the microphone: {e}"))?;
    let _ = ready.send(Ok(()));

    let tick = Duration::from_millis(1000 / METER_HZ);
    loop {
        match stop.recv_timeout(tick) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let n = samples.swap(0, Ordering::Relaxed);
        let s = f32::from_bits(sum_sq.swap(0.0f32.to_bits(), Ordering::Relaxed));
        let p = f32::from_bits(peak.swap(0.0f32.to_bits(), Ordering::Relaxed));
        let rms = if n > 0 { (s / n as f32).sqrt() } else { 0.0 };
        let _ = app.emit("voice-input-level", Level { rms, peak: p });
    }

    // Dropping the stream here, on the thread that built it, is the point of this thread.
    drop(stream);
    let _ = app.emit("voice-input-level", Level { rms: 0.0, peak: 0.0 });
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Output test
// ---------------------------------------------------------------------------------------

/// Play a short tone on the configured output, so the player can confirm which headset
/// voice will actually come out of before they're on a grid with twenty people.
pub fn test_output(device: &str, volume: f32) -> Result<Option<String>, String> {
    let (device, warning) = resolve(device, false)?;
    let config = device
        .default_output_config()
        .map_err(|e| format!("Couldn't read the output's format: {e}"))?;
    let sample_rate = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;
    let volume = volume.clamp(0.0, 1.0);

    let (done_tx, done_rx) = mpsc::channel::<Result<(), String>>();
    std::thread::spawn(move || {
        let mut phase = 0.0f32;
        let mut elapsed = 0u64;
        let total = (sample_rate as u64 * TEST_TONE_MS) / 1000;
        let err_fn = |e| log::warn!("voice: output stream error: {e}");

        let built = device.build_output_stream(
            &config.config(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for frame in data.chunks_mut(channels) {
                    // Fade both ends: a tone that starts and stops at full amplitude
                    // clicks, and a click in headphones is unpleasant at any volume.
                    let progress = elapsed as f32 / total.max(1) as f32;
                    let envelope = (progress * 12.0).min(1.0).min((1.0 - progress) * 12.0).max(0.0);
                    let v = (phase * std::f32::consts::TAU).sin() * TEST_TONE_ADSR_PEAK * envelope * volume;
                    for slot in frame.iter_mut() {
                        *slot = v;
                    }
                    phase = (phase + 440.0 / sample_rate).fract();
                    elapsed += 1;
                }
            },
            err_fn,
            None,
        );

        let stream = match built {
            Ok(s) => s,
            Err(e) => {
                let _ = done_tx.send(Err(format!("Couldn't open the output: {e}")));
                return;
            }
        };
        if let Err(e) = stream.play() {
            let _ = done_tx.send(Err(format!("Couldn't start the output: {e}")));
            return;
        }
        let _ = done_tx.send(Ok(()));
        std::thread::sleep(Duration::from_millis(TEST_TONE_MS + 120));
        drop(stream);
    });

    match done_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(warning),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("The output didn't start in time.".to_string()),
    }
}

// ---------------------------------------------------------------------------------------
// The mic key: push to talk, or toggle
// ---------------------------------------------------------------------------------------

/// Bind the mic key.
///
/// Called from [`crate::overlay::register`], which owns global-shortcut registration for the
/// whole app: it clears every binding before rebinding, so a mic key registered anywhere
/// else would be wiped the next time the overlay hotkey changed.
///
/// Two modes, and the difference is entirely in which edges matter:
///
/// - **Push to talk** acts on *both* edges — the mic opens on press and closes on release.
/// - **Toggle** acts on the press only, flipping the mic each time. The release is ignored,
///   because acting on it is what would make the key a push-to-talk key.
///
/// The mic always starts closed on a rebind. That matters more for toggle: the state lives
/// in this process, so a rebind (or turning voice off and on) must not leave someone
/// transmitting with no key press behind it.
pub fn bind_ptt<R: Runtime>(
    app: &AppHandle<R>,
    cfg: &crate::config::AppConfig,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    PTT_DOWN.store(false, Ordering::Relaxed);
    let _ = app.emit("voice-ptt", false);
    if !cfg.voice_enabled {
        return Ok(());
    }
    let combo = ptt_hotkey_of(cfg).to_string();
    let shortcut: tauri_plugin_global_shortcut::Shortcut = combo
        .parse()
        .map_err(|_| format!("\"{combo}\" isn't a key combination we understand."))?;

    let toggle = cfg.voice_toggle_to_talk;
    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            let pressed = event.state() == ShortcutState::Pressed;
            let open = if toggle {
                // Only the press flips it; the release is what we are deliberately not
                // acting on. A held key also repeats presses, so ignore a repeat that
                // arrives while we already believe the key is down.
                if !pressed {
                    KEY_HELD.store(false, Ordering::Relaxed);
                    return;
                }
                if KEY_HELD.swap(true, Ordering::Relaxed) {
                    return; // auto-repeat, not a new press
                }
                !PTT_DOWN.load(Ordering::Relaxed)
            } else {
                KEY_HELD.store(pressed, Ordering::Relaxed);
                pressed
            };
            // Nothing to say if it didn't actually change.
            if PTT_DOWN.swap(open, Ordering::Relaxed) == open {
                return;
            }
            let _ = handle.emit("voice-ptt", open);
        })
        .map_err(|e| {
            format!("Couldn't register {combo} for the mic key — another app is probably using it. ({e})")
        })?;
    log::info!(
        "voice: mic key bound to {combo} ({})",
        if toggle { "toggle" } else { "push to talk" }
    );
    Ok(())
}

/// The configured PTT combo, or the default when blank.
pub fn ptt_hotkey_of(cfg: &crate::config::AppConfig) -> &str {
    let key = cfg.voice_ptt_hotkey.trim();
    if key.is_empty() {
        crate::config::DEFAULT_PTT_HOTKEY
    } else {
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, DEFAULT_PTT_HOTKEY};

    #[test]
    fn a_blank_ptt_hotkey_falls_back_to_the_default() {
        // Blank is what a config written before voice existed carries, and what the UI
        // sends when a player clears the field.
        let mut cfg = AppConfig::default();
        cfg.voice_ptt_hotkey = String::new();
        assert_eq!(ptt_hotkey_of(&cfg), DEFAULT_PTT_HOTKEY);
        cfg.voice_ptt_hotkey = "   ".into();
        assert_eq!(ptt_hotkey_of(&cfg), DEFAULT_PTT_HOTKEY);
    }

    #[test]
    fn a_set_ptt_hotkey_is_used_verbatim() {
        let mut cfg = AppConfig::default();
        cfg.voice_ptt_hotkey = "Alt+V".into();
        assert_eq!(ptt_hotkey_of(&cfg), "Alt+V");
    }

    #[test]
    fn enumeration_never_panics_and_never_repeats_a_device() {
        // CI runners have no sound card; the contract is that this degrades to an empty
        // list with an error, rather than failing the call.
        let d = devices();
        for list in [&d.inputs, &d.outputs] {
            let mut names: Vec<&str> = list.iter().map(|x| x.name.as_str()).collect();
            let before = names.len();
            names.sort_unstable();
            names.dedup();
            assert_eq!(before, names.len(), "duplicate device names would be unpickable");
        }
        if d.inputs.is_empty() && d.outputs.is_empty() {
            assert!(d.error.is_some(), "no devices must be reported, not shown as empty pickers");
        }
    }

    #[test]
    fn at_most_one_device_per_list_is_the_default() {
        let d = devices();
        for list in [&d.inputs, &d.outputs] {
            assert!(list.iter().filter(|x| x.is_default).count() <= 1);
        }
    }

    #[test]
    fn a_missing_named_device_is_reported_rather_than_swallowed() {
        // The unplugged-headset case. Either we have no audio at all (Err), or we fall
        // back to the default and say why — silently going mute is the one wrong answer.
        match resolve("no such device ~~~", true) {
            Ok((_, warning)) => {
                let w = warning.expect("falling back to the default must be reported");
                assert!(w.contains("no such device"), "unhelpful warning: {w}");
            }
            Err(e) => assert!(e.contains("microphone"), "unhelpful error: {e}"),
        }
    }

    #[test]
    fn a_blank_device_resolves_quietly_to_the_default() {
        // Blank means "follow the system default", so it is not a fallback and must not warn.
        if let Ok((_, warning)) = resolve("", false) {
            assert!(warning.is_none(), "the default is the setting, not a substitution");
        }
    }

    #[test]
    fn ptt_starts_closed() {
        assert!(!transmitting(), "the mic must never be open before a key is pressed");
    }

    #[test]
    fn push_to_talk_is_the_default() {
        // The mode that cannot leave a microphone open by accident is the one you get
        // without choosing.
        assert!(!AppConfig::default().voice_toggle_to_talk);
    }

    /// The mic-state decision, lifted out of the shortcut handler so the two modes can be
    /// exercised without a global shortcut — which macOS can't deliver to us anyway.
    fn next_open(toggle: bool, pressed: bool, key_held: &mut bool, open: bool) -> Option<bool> {
        if toggle {
            if !pressed {
                *key_held = false;
                return None;
            }
            if std::mem::replace(key_held, true) {
                return None; // auto-repeat
            }
            Some(!open)
        } else {
            *key_held = pressed;
            Some(pressed)
        }
    }

    #[test]
    fn push_to_talk_opens_on_press_and_closes_on_release() {
        let mut held = false;
        assert_eq!(next_open(false, true, &mut held, false), Some(true));
        assert_eq!(next_open(false, false, &mut held, true), Some(false));
    }

    #[test]
    fn toggle_latches_and_ignores_the_release() {
        let mut held = false;
        // Press opens...
        assert_eq!(next_open(true, true, &mut held, false), Some(true));
        // ...and the release must NOT close it, or it's push-to-talk again.
        assert_eq!(next_open(true, false, &mut held, true), None);
        // The next press closes.
        assert_eq!(next_open(true, true, &mut held, true), Some(false));
        assert_eq!(next_open(true, false, &mut held, false), None);
    }

    #[test]
    fn a_held_key_does_not_flap_the_toggle() {
        // The OS repeats a held key. Acting on each repeat would open and close the mic
        // dozens of times a second for as long as someone leans on it.
        let mut held = false;
        assert_eq!(next_open(true, true, &mut held, false), Some(true));
        for _ in 0..10 {
            assert_eq!(next_open(true, true, &mut held, true), None, "repeat must be ignored");
        }
    }
}
