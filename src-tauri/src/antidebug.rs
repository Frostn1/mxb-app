//! Refuse to run under a debugger — release builds only.
//!
//! The release profile in `Cargo.toml` spends a lot to make the shipped binary hard to
//! read at rest: symbols stripped, fat LTO to dissolve the call structure, one codegen
//! unit, no debug info. All of that is defeated the moment someone attaches a live
//! debugger to the running process and watches it work — breakpoints on the paint
//! decrypt, a peek at the cf_clearance the session is holding, a step through the IPC
//! guard. This is the runtime half of that hardening: if a usermode debugger is present,
//! the process quietly exits instead of handing over a live target.
//!
//! Debug builds are deliberately exempt (`cfg!(debug_assertions)`), for the same reason
//! the single-instance guard and close-to-tray are — a `cargo test` or a `tauri dev` run
//! must stay debuggable, and those always carry `debug_assertions`. So every detector
//! below is compiled only into release builds; the whole module is a no-op otherwise.
//!
//! The check is best-effort by nature — a determined reverse engineer can patch any of
//! these out — but it raises the cost from "attach and watch" to "find and neutralise the
//! guard first", which is the whole game with anti-tamper.

/// Install the debugger guard. Call once, as early in `main` as possible.
///
/// In release builds this checks for a debugger now and, if none is attached yet, leaves a
/// low-frequency watcher running so a debugger attached *after* launch — the usual way, by
/// attaching to `MXB App.exe` once it is up — is caught too. In debug builds it does nothing.
pub fn guard() {
    #[cfg(not(debug_assertions))]
    {
        if debugger_present() {
            bail();
        }
        // Attaching to a process that is already running is the common case (open x64dbg,
        // pick `MXB App.exe`), so a one-shot check at startup would miss the very attack it
        // is meant to stop. A slow poll costs nothing measurable and closes that window.
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if debugger_present() {
                bail();
            }
        });
    }
}

/// Exit without ceremony. No log line and no dialog: a message would tell whoever is poking
/// at the binary exactly which check fired and where to patch, and the release build has no
/// console to print it to anyway. Exit status 0 so it reads as an ordinary clean shutdown.
#[cfg(not(debug_assertions))]
fn bail() -> ! {
    std::process::exit(0);
}

/// True when a usermode debugger is attached to this process.
#[cfg(all(not(debug_assertions), target_os = "windows"))]
fn debugger_present() -> bool {
    // Two questions, because they catch different attachers. `IsDebuggerPresent` reads the
    // PEB's `BeingDebugged` flag — set when a debugger launched or attached the normal way.
    // `CheckRemoteDebuggerPresent` asks the kernel about this process's debug port, which
    // also catches a debugger that cleared `BeingDebugged` to hide itself.
    #[link(name = "kernel32")]
    extern "system" {
        fn IsDebuggerPresent() -> i32;
        fn GetCurrentProcess() -> isize;
        fn CheckRemoteDebuggerPresent(process: isize, present: *mut i32) -> i32;
    }

    unsafe {
        if IsDebuggerPresent() != 0 {
            return true;
        }
        let mut present: i32 = 0;
        if CheckRemoteDebuggerPresent(GetCurrentProcess(), &mut present) != 0 && present != 0 {
            return true;
        }
    }
    false
}

/// True when another process is `ptrace`-attached to this one (gdb, strace, a debugger).
#[cfg(all(not(debug_assertions), target_os = "linux"))]
fn debugger_present() -> bool {
    // The kernel reports the tracer's pid in `/proc/self/status`; non-zero means traced.
    std::fs::read_to_string("/proc/self/status")
        .map(|status| tracer_attached(&status))
        .unwrap_or(false)
}

/// True when this process carries the `P_TRACED` flag — i.e. a debugger is attached.
#[cfg(all(not(debug_assertions), target_os = "macos"))]
fn debugger_present() -> bool {
    // Apple's own recipe (QA1361): ask the kernel for this process's `kinfo_proc` via sysctl
    // and read the `P_TRACED` bit out of `kp_proc.p_flag`. `libc` doesn't expose `kinfo_proc`
    // on Apple, and the full struct is large and version-sensitive, so rather than model it
    // we read a generous zeroed buffer and pull `p_flag` from its fixed offset: `kp_proc`
    // (an `extern_proc`) sits at the front of `kinfo_proc`, and on 64-bit `p_flag` is the
    // `int` at byte 32 — past the 16-byte `p_un` union and the two 8-byte pointers before it.
    use std::os::raw::{c_int, c_uint, c_void};

    // These live in libSystem, which every macOS binary links against.
    extern "C" {
        fn sysctl(
            name: *mut c_int,
            namelen: c_uint,
            oldp: *mut c_void,
            oldlenp: *mut usize,
            newp: *mut c_void,
            newlen: usize,
        ) -> c_int;
        fn getpid() -> c_int;
    }

    const CTL_KERN: c_int = 1;
    const KERN_PROC: c_int = 14;
    const KERN_PROC_PID: c_int = 1;
    const P_TRACED: i32 = 0x0000_0800;
    const P_FLAG_OFFSET: usize = 32;

    unsafe {
        // `kinfo_proc` is 648 bytes on current macOS; a kilobyte leaves generous headroom for
        // any layout drift without ever under-sizing the query (which would just fail cleanly).
        let mut buf = [0u8; 1024];
        let mut len = buf.len();
        let mut mib = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()];
        let ret = sysctl(
            mib.as_mut_ptr(),
            mib.len() as c_uint,
            buf.as_mut_ptr() as *mut c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        );
        // A failed or too-short query is not evidence of a debugger — don't kill the app over it.
        if ret != 0 || len < P_FLAG_OFFSET + 4 {
            return false;
        }
        let p_flag = i32::from_ne_bytes(buf[P_FLAG_OFFSET..P_FLAG_OFFSET + 4].try_into().unwrap());
        (p_flag & P_TRACED) != 0
    }
}

/// Anywhere else, there is no check to make — never report a debugger.
#[cfg(all(
    not(debug_assertions),
    not(any(target_os = "windows", target_os = "linux", target_os = "macos"))
))]
fn debugger_present() -> bool {
    false
}

/// Parse the `TracerPid` line out of a `/proc/<pid>/status` body: non-zero means some
/// process is `ptrace`-attached. Split out from the filesystem read so it can be tested.
#[cfg(any(test, all(not(debug_assertions), target_os = "linux")))]
fn tracer_attached(status: &str) -> bool {
    status.lines().any(|line| {
        line.strip_prefix("TracerPid:")
            .map(str::trim)
            .is_some_and(|pid| pid != "0")
    })
}

#[cfg(test)]
mod tests {
    use super::tracer_attached;

    /// The line the kernel writes when nothing is attached — the overwhelmingly common case,
    /// and the one that must never trip the guard.
    #[test]
    fn a_zero_tracer_pid_is_not_a_debugger() {
        let status = "Name:\tfrost\nState:\tR (running)\nTracerPid:\t0\nUid:\t1000\n";
        assert!(!tracer_attached(status));
    }

    /// gdb/strace attach shows up here as the tracer's pid.
    #[test]
    fn a_nonzero_tracer_pid_is_a_debugger() {
        let status = "Name:\tfrost\nState:\tt (tracing stop)\nTracerPid:\t4242\nUid:\t1000\n";
        assert!(tracer_attached(status));
    }

    /// No `TracerPid` line at all (a truncated or unexpected `status`) is not evidence of a
    /// debugger — err toward letting the app run.
    #[test]
    fn a_missing_tracer_line_is_not_a_debugger() {
        assert!(!tracer_attached("Name:\tfrost\nState:\tR (running)\n"));
    }
}
