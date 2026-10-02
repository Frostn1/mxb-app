//! Last words for a process that is about to die, written to the app's own log.
//!
//! The release profile builds with `panic = "abort"`, and a release build has no console, so
//! without this a panic anywhere (a startup task, a command, a watcher thread) ends MXB App
//! with nothing in `MXB App.log` at all: the log just stops, and the player relaunches into the
//! same thing. The same goes for a native crash (an access violation in a DLL the app loaded),
//! which never reaches Rust's panic machinery.
//!
//! [`install`] adds two hooks that each write one ERROR line through the normal logger and
//! flush it before the process goes:
//! - a panic hook: the message, where it was raised, and which thread;
//! - on Windows, an unhandled-exception filter: the exception code, the faulting address, and
//!   the module that holds it (`mxbsecure.dll`, `WebView2Loader.dll`, ...).
//!
//! Both chain to whatever was there before, so the default panic message still reaches stderr
//! in a dev run and Windows Error Reporting still sees the crash.

/// Install both hooks. Call once, first thing in `main`; lines written before the logger is up
/// go nowhere, which only affects a panic before Tauri has started.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("(no message)");
        let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let thread = std::thread::current();
        log::error!("{}", describe_panic(thread.name(), message, location.as_deref()));
        log::logger().flush();
        previous(info);
    }));
    #[cfg(windows)]
    native::install();
}

/// The line a panic leaves in the log.
fn describe_panic(thread: Option<&str>, message: &str, location: Option<&str>) -> String {
    format!(
        "[crash] panic in thread '{}' at {}: {} (the app is closing)",
        thread.unwrap_or("unnamed"),
        location.unwrap_or("an unknown location"),
        message
    )
}

/// The line a native crash leaves in the log.
#[cfg_attr(not(windows), allow(dead_code))]
fn describe_native(code: u32, address: usize, module: Option<&str>) -> String {
    format!(
        "[crash] native exception {code:#010x} at {address:#x} in {} (the app is closing)",
        module.unwrap_or("an unknown module")
    )
}

#[cfg(windows)]
mod native {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[repr(C)]
    struct ExceptionRecord {
        code: u32,
        flags: u32,
        record: *mut ExceptionRecord,
        address: *mut c_void,
    }

    #[repr(C)]
    struct ExceptionPointers {
        record: *mut ExceptionRecord,
        context: *mut c_void,
    }

    type Filter = unsafe extern "system" fn(*const ExceptionPointers) -> i32;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetUnhandledExceptionFilter(filter: Option<Filter>) -> Option<Filter>;
        fn GetModuleHandleExW(flags: u32, address: *const u16, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, filename: *mut u16, size: u32) -> u32;
    }

    const GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT: u32 = 0x2;
    const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: u32 = 0x4;
    const EXCEPTION_CONTINUE_SEARCH: i32 = 0;

    /// The filter that was installed before ours, called after we have logged.
    static PREVIOUS: AtomicUsize = AtomicUsize::new(0);

    pub fn install() {
        // SAFETY: installs a process-wide filter; `on_crash` matches the signature Windows calls.
        let previous = unsafe { SetUnhandledExceptionFilter(Some(on_crash)) };
        PREVIOUS.store(previous.map_or(0, |f| f as usize), Ordering::SeqCst);
    }

    /// The file name of the module that contains `address`, if any does.
    fn module_of(address: *mut c_void) -> Option<String> {
        let mut module = std::ptr::null_mut();
        // SAFETY: FROM_ADDRESS reads `address` as a code address, not a string; no refcount.
        let found = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                address as *const u16,
                &mut module,
            )
        };
        if found == 0 {
            return None;
        }
        let mut buf = [0u16; 520];
        // SAFETY: a live module handle and a fixed local buffer.
        let n = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if n == 0 || n >= buf.len() {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..n]);
        Some(path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string())
    }

    unsafe extern "system" fn on_crash(pointers: *const ExceptionPointers) -> i32 {
        // SAFETY: Windows hands the filter a valid EXCEPTION_POINTERS for the faulting thread.
        let record = unsafe { pointers.as_ref().and_then(|p| p.record.as_ref()) };
        if let Some(record) = record {
            let module = module_of(record.address);
            log::error!("{}", super::describe_native(record.code, record.address as usize, module.as_deref()));
            log::logger().flush();
        }
        match PREVIOUS.load(Ordering::SeqCst) {
            0 => EXCEPTION_CONTINUE_SEARCH,
            // SAFETY: the value `SetUnhandledExceptionFilter` returned, a filter of this type.
            f => unsafe { std::mem::transmute::<usize, Filter>(f)(pointers) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_names_its_thread_place_and_message() {
        assert_eq!(
            describe_panic(Some("tokio-runtime-worker"), "boom", Some("src/main.rs:10:5")),
            "[crash] panic in thread 'tokio-runtime-worker' at src/main.rs:10:5: boom (the app is closing)"
        );
        assert!(describe_panic(None, "x", None).contains("'unnamed' at an unknown location"));
    }

    #[test]
    fn a_native_crash_names_the_module_it_happened_in() {
        assert_eq!(
            describe_native(0xC000_0005, 0x7ff8_1234, Some("mxbsecure.dll")),
            "[crash] native exception 0xc0000005 at 0x7ff81234 in mxbsecure.dll (the app is closing)"
        );
    }

    /// The hook itself, end to end: a panic on another thread goes through it and the thread
    /// still reports the panic to whoever joins it (tests run with unwinding).
    #[test]
    fn the_hook_lets_a_panic_through() {
        install();
        let joined = std::thread::Builder::new()
            .name("crashlog-test".into())
            .spawn(|| panic!("on purpose"))
            .unwrap()
            .join();
        assert!(joined.is_err());
    }
}
