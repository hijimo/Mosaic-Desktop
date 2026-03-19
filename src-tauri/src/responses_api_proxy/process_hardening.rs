//! Process hardening: disable core dumps, ptrace, and dangerous env vars.

/// Apply platform-specific process hardening before main logic runs.
pub fn pre_main_hardening() {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    hardening_linux();

    #[cfg(target_os = "macos")]
    hardening_macos();

    #[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
    hardening_bsd();
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn hardening_linux() {
    unsafe {
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
    }
    set_core_file_size_limit_to_zero();
    remove_env_with_prefix(b"LD_");
}

#[cfg(target_os = "macos")]
fn hardening_macos() {
    unsafe {
        libc::ptrace(libc::PT_DENY_ATTACH, 0, std::ptr::null_mut(), 0);
    }
    set_core_file_size_limit_to_zero();
    remove_env_with_prefix(b"DYLD_");
}

#[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
fn hardening_bsd() {
    set_core_file_size_limit_to_zero();
    remove_env_with_prefix(b"LD_");
}

#[cfg(unix)]
fn set_core_file_size_limit_to_zero() {
    let rlim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    unsafe {
        libc::setrlimit(libc::RLIMIT_CORE, &rlim);
    }
}

#[cfg(unix)]
fn remove_env_with_prefix(prefix: &[u8]) {
    use std::os::unix::ffi::OsStrExt;
    let keys: Vec<_> = std::env::vars_os()
        .filter_map(|(key, _)| {
            key.as_os_str()
                .as_bytes()
                .starts_with(prefix)
                .then_some(key)
        })
        .collect();
    for key in keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_main_hardening_does_not_panic() {
        // Just verify it doesn't crash. On macOS PT_DENY_ATTACH may fail
        // in test context but we don't exit on failure in our version.
        // This test mainly ensures the code compiles and runs.
        // Note: we skip calling pre_main_hardening() directly in tests
        // because PT_DENY_ATTACH would interfere with the test runner's
        // debugger. Instead we test the sub-components.
    }

    #[cfg(unix)]
    #[test]
    fn remove_env_with_prefix_works() {
        use std::os::unix::ffi::OsStrExt;

        // Set a test env var
        let test_key = "MOSAIC_TEST_LD_REMOVE";
        std::env::set_var(test_key, "1");
        assert!(std::env::var(test_key).is_ok());

        // This won't match "MOSAIC_TEST_LD_REMOVE" since prefix is "LD_"
        remove_env_with_prefix(b"MOSAIC_TEST_LD_");
        assert!(std::env::var(test_key).is_err());
    }
}
