//! Where a voice comes from.
//!
//! This is the payoff, and it is the part nobody else can build: it needs the rider table
//! and the audio pipeline in the same process, and we have both. A rider two corners back
//! is faint and behind you; the one you are about to land on is loud and to your left. You
//! learn where people are without looking, which is what a spotter is for.
//!
//! **Nothing positional crosses the network.** Every app already holds every rider's
//! position — the game gives it to the plugin, the plugin publishes it, the app reads it —
//! so the scene is computed at the listener's end from data that was already there.
//! Proximity therefore costs no bandwidth at all, and a rider whose FrostMod isn't running
//! hears everyone flat rather than hearing nothing.
//!
//! ## The conventions, and where they come from
//!
//! The ground plane is (X, Z) with Y up, and yaw is degrees from north — settled against
//! PiBoSo's SDK header and cross-checked against mxbmrp3. The rotation below is the radar's,
//! deliberately: the radar draws a blip where this pans a voice, so if one is ever wrong the
//! other is visibly wrong in the same direction, on screen, where it is cheap to notice.

/// Inside this, a rider is at full volume. Roughly the length of a start gate — close
/// enough that shouting at each other should not be attenuated at all.
pub const NEAR_METRES: f32 = 6.0;

/// Past this, silence. A voice from the other side of the circuit is noise: you cannot act
/// on it, and forty riders all faintly audible is worse than a quiet channel.
pub const FAR_METRES: f32 = 150.0;

/// How much of the original signal survives at the extremes of the stereo image. A voice
/// hard left is not *silent* on the right — that is disorienting on headphones — it is
/// quieter, which is what the ear actually uses.
const MIN_CHANNEL: f32 = 0.15;

/// Where the listener is and which way they are facing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Degrees from north, exactly as the game reports it.
    pub yaw_deg: f32,
}

/// How loud, and how far to each side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Gain for the left channel, 0..1.
    pub left: f32,
    /// Gain for the right channel, 0..1.
    pub right: f32,
}

impl Placement {
    /// Both ears, equally — what a rider hears when there is nothing to place them by.
    pub const FLAT: Placement = Placement { left: 1.0, right: 1.0 };

    pub fn is_silent(&self) -> bool {
        self.left <= 0.0 && self.right <= 0.0
    }
}

/// Place a voice relative to the listener.
///
/// Returns silence past [`FAR_METRES`], which is also the sender-side cull: the same
/// function decides whether a frame is worth transmitting at all, so a rider who cannot be
/// heard is never sent to. That is what keeps a full grid affordable — twenty riders is
/// nineteen streams if everyone talks at once, and three or four if only the ones near you
/// do.
pub fn place(listener: Listener, x: f32, _y: f32, z: f32) -> Placement {
    let (forward, right, distance) = relative(listener, x, z);

    let gain = distance_gain(distance);
    if gain <= 0.0 {
        return Placement { left: 0.0, right: 0.0 };
    }

    // Bearing: 0 straight ahead, +right, ±π behind. Matching the radar's `atan2(rx, ry)`.
    let bearing = right.atan2(forward);
    // A rider directly behind pans to centre, the same as one directly ahead. Two speakers
    // cannot say front from back without head tracking, and pretending otherwise by
    // flipping the image at 90° makes a rider you are passing lurch across your head.
    let pan = bearing.sin().clamp(-1.0, 1.0);

    // Equal power: left² + right² stays constant across the sweep, so a rider crossing in
    // front of you doesn't dip in the middle the way a linear pan does.
    let angle = (pan + 1.0) * (std::f32::consts::FRAC_PI_4);
    let left = angle.cos().max(MIN_CHANNEL);
    let right = angle.sin().max(MIN_CHANNEL);

    Placement { left: left * gain, right: right * gain }
}

/// Is this rider close enough to be worth sending to at all?
pub fn in_range(listener: Listener, x: f32, z: f32) -> bool {
    let (_, _, distance) = relative(listener, x, z);
    distance < FAR_METRES
}

/// The listener-relative geometry: how far ahead, how far right, how far away.
fn relative(listener: Listener, x: f32, z: f32) -> (f32, f32, f32) {
    let du = x - listener.x;
    let dv = z - listener.z;
    let distance = (du * du + dv * dv).sqrt();

    let a = listener.yaw_deg.to_radians();
    let (sin_a, cos_a) = a.sin_cos();
    // The radar's rotation, sign for sign: rx is rightward, ry is ahead.
    let right = du * cos_a - dv * sin_a;
    let forward = du * sin_a + dv * cos_a;
    (forward, right, distance)
}

/// Volume from distance.
///
/// An inverse rolloff — how sound actually behaves — faded to nothing at the cutoff so a
/// voice doesn't vanish mid-word when someone crosses the range boundary. Linear would be
/// simpler and wrong: it keeps distant riders far too present, which is exactly the
/// complaint about proximity chat that doesn't model falloff.
fn distance_gain(distance: f32) -> f32 {
    if distance <= NEAR_METRES {
        return 1.0;
    }
    if distance >= FAR_METRES {
        return 0.0;
    }
    let rolloff = NEAR_METRES / distance;
    let fade = (FAR_METRES - distance) / (FAR_METRES - NEAR_METRES);
    (rolloff * fade).clamp(0.0, 1.0)
}

/// Move a gain toward its target, one frame at a time.
///
/// Closing speeds on a track are high — two riders can go from 40 m apart to alongside in
/// under a second — and a gain that jumped every 20 ms would chatter audibly. This is a
/// one-pole filter: fast enough to track a pass, slow enough that the steps are inaudible.
pub fn glide(current: f32, target: f32) -> f32 {
    // ~60 ms to cover most of the distance at 50 frames a second.
    const RATE: f32 = 0.25;
    current + (target - current) * RATE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Facing north (0°), at the origin.
    const ME: Listener = Listener { x: 0.0, y: 0.0, z: 0.0, yaw_deg: 0.0 };

    fn approx(a: f32, b: f32, tolerance: f32) -> bool {
        (a - b).abs() < tolerance
    }

    #[test]
    fn someone_beside_you_is_at_full_volume() {
        // Full *power*, not full in both ears: at 2 m to your right they are hard-panned
        // right, which is the whole point. Loudness is what distance decides.
        let p = place(ME, 2.0, 0.0, 0.0);
        let power = (p.left * p.left + p.right * p.right).sqrt();
        assert!(power > 0.95, "{p:?} is only {power} loud");
        assert!(p.right > p.left, "and should still favour the ear they are on: {p:?}");
    }

    #[test]
    fn someone_across_the_circuit_is_silent() {
        let p = place(ME, FAR_METRES + 1.0, 0.0, 0.0);
        assert!(p.is_silent(), "{p:?}");
        assert!(!in_range(ME, FAR_METRES + 1.0, 0.0), "and is never sent to");
    }

    #[test]
    fn further_away_is_quieter_all_the_way_out() {
        // Monotonic: no distance where moving away makes someone louder.
        let mut previous = f32::MAX;
        for metres in 1..(FAR_METRES as i32) {
            let p = place(ME, metres as f32, 0.0, 0.0);
            let loudness = p.left + p.right;
            assert!(loudness <= previous + 1e-6, "louder at {metres} m than at {}", metres - 1);
            previous = loudness;
        }
    }

    #[test]
    fn a_rider_on_your_left_is_louder_on_the_left() {
        // Facing north (+Z ahead), so -X is to the left.
        let left_side = place(ME, -20.0, 0.0, 0.0);
        assert!(left_side.left > left_side.right, "{left_side:?}");

        let right_side = place(ME, 20.0, 0.0, 0.0);
        assert!(right_side.right > right_side.left, "{right_side:?}");
    }

    #[test]
    fn the_image_follows_your_heading_not_the_map() {
        // Same rider, but now you are facing east: what was on your right is ahead of you.
        let facing_east = Listener { yaw_deg: 90.0, ..ME };
        let p = place(facing_east, 20.0, 0.0, 0.0);
        assert!(approx(p.left, p.right, 0.05), "a rider straight ahead should be centred: {p:?}");
    }

    #[test]
    fn a_rider_ahead_and_one_behind_are_both_centred() {
        // Two speakers cannot say front from back. Centred is the honest answer; flipping
        // the image at 90° would make a rider you are passing lurch across your head.
        let ahead = place(ME, 0.0, 0.0, 20.0);
        let behind = place(ME, 0.0, 0.0, -20.0);
        assert!(approx(ahead.left, ahead.right, 0.01), "{ahead:?}");
        assert!(approx(behind.left, behind.right, 0.01), "{behind:?}");
    }

    #[test]
    fn panning_keeps_its_power_across_the_sweep() {
        // Equal power: a rider crossing in front of you must not dip in the middle.
        let mut loudness = Vec::new();
        for degrees in (0..360).step_by(15) {
            let a = (degrees as f32).to_radians();
            let (x, z) = (a.sin() * 10.0, a.cos() * 10.0);
            let p = place(ME, x, 0.0, z);
            loudness.push((p.left * p.left + p.right * p.right).sqrt());
        }
        let min = loudness.iter().cloned().fold(f32::MAX, f32::min);
        let max = loudness.iter().cloned().fold(0.0, f32::max);
        assert!(max - min < 0.12, "power swings from {min} to {max} around the circle");
    }

    #[test]
    fn a_voice_is_never_completely_gone_from_one_ear() {
        // Hard-panned voices are disorienting on headphones, which is what everyone uses.
        let p = place(ME, -6.0, 0.0, 0.0);
        assert!(p.left > 0.0 && p.right > 0.0, "{p:?}");
    }

    #[test]
    fn height_does_not_change_the_image() {
        // A rider on a jump is not panned differently for being in the air: the game's
        // ground plane is (X, Z), and folding Y in would make a tabletop swing the stereo.
        let ground = place(ME, 10.0, 0.0, 10.0);
        let airborne = place(ME, 10.0, 8.0, 10.0);
        assert_eq!(ground, airborne);
    }

    #[test]
    fn a_gain_glides_rather_than_jumping() {
        let mut gain = 0.0;
        for _ in 0..40 {
            gain = glide(gain, 1.0);
        }
        assert!(gain > 0.99, "should reach the target within a second, got {gain}");

        // And no single step is big enough to click.
        let step = glide(0.0, 1.0);
        assert!(step < 0.4, "one step moved {step}");
    }

    #[test]
    fn the_cull_and_the_mix_agree_about_range() {
        // The sender skips anyone out of range; the mixer silences them. If those two ever
        // disagreed, a rider would be sent audio that was thrown away, or worse, culled
        // while still expected.
        for metres in [1.0, 50.0, FAR_METRES - 1.0, FAR_METRES, FAR_METRES + 50.0] {
            let audible = !place(ME, metres, 0.0, 0.0).is_silent();
            assert_eq!(audible, in_range(ME, metres, 0.0), "at {metres} m");
        }
    }
}
