#![cfg(target_os = "macos")]
//! macOS Seatbelt (sandbox-exec) integration.
//!
//! Generates Seatbelt profiles from `SandboxPolicy` and spawns child processes
//! under the macOS sandbox. Only compiled on macOS targets.

use std::collections::HashMap;
use std::ffi::CStr;
use std::path::{Path, PathBuf};

use crate::protocol::types::SandboxPolicy;

/// Path to the system `sandbox-exec` binary. Only trust the one in `/usr/bin`
/// to defend against PATH injection.
pub const MACOS_PATH_TO_SEATBELT_EXECUTABLE: &str = "/usr/bin/sandbox-exec";

const SEATBELT_BASE_POLICY: &str = r#"(version 1)
(deny default)

; allow basic process operations
(allow process-exec*)
(allow process-fork)
(allow signal (target self))

; allow reading system libraries and frameworks
(allow file-read*
    (subpath "/usr/lib")
    (subpath "/usr/share")
    (subpath "/System")
    (subpath "/Library/Frameworks")
    (subpath "/private/var/db/dyld")
    (subpath "/dev")
    (literal "/etc")
    (literal "/tmp")
    (literal "/var")
    (literal "/private")
    (literal "/private/tmp")
    (literal "/private/var")
    (literal "/private/var/tmp")
)

; allow sysctl reads for os.cpus() etc.
(allow sysctl-read
    (sysctl-name "machdep.cpu.brand_string")
    (sysctl-name "hw.model")
    (sysctl-name "hw.ncpu")
    (sysctl-name "hw.memsize")
    (sysctl-name "kern.ostype")
    (sysctl-name "kern.osrelease")
    (sysctl-name "kern.hostname")
)

; allow mach IPC for basic system services
(allow mach-lookup
    (global-name "com.apple.system.logger")
    (global-name "com.apple.system.notification_center")
)

; allow writing to /dev/null, /dev/tty, etc.
(allow file-write*
    (subpath "/dev")
)
"#;

const SEATBELT_NETWORK_POLICY: &str = r#"
; allow DNS resolution
(allow network-outbound (remote unix-socket (path-literal "/var/run/mDNSResponder")))
(allow system-socket)
"#;

const SEATBELT_PLATFORM_DEFAULTS: &str = r#"
; macOS permission profile extensions
(allow ipc-posix-shm-read* (ipc-posix-name-prefix "apple.cfprefs."))
(allow mach-lookup
    (global-name "com.apple.cfprefsd.daemon")
    (global-name "com.apple.cfprefsd.agent")
    (local-name "com.apple.cfprefsd.agent"))
(allow user-preference-read)
"#;

/// Build the full `sandbox-exec` argument list for a command under the given policy.
///
/// Returns a `Vec<String>` suitable for passing to `Command::new(MACOS_PATH_TO_SEATBELT_EXECUTABLE).args(...)`.
pub fn create_seatbelt_command_args(
    command: Vec<String>,
    sandbox_policy: &SandboxPolicy,
    sandbox_policy_cwd: &Path,
) -> Vec<String> {
    let (file_write_policy, file_write_dir_params) =
        build_write_policy(sandbox_policy, sandbox_policy_cwd);
    let (file_read_policy, file_read_dir_params) =
        build_read_policy(sandbox_policy, sandbox_policy_cwd);
    let network_policy = build_network_policy(sandbox_policy);

    let mut policy_sections = vec![
        SEATBELT_BASE_POLICY.to_string(),
        file_read_policy,
        file_write_policy,
        network_policy,
    ];

    if matches!(
        sandbox_policy,
        SandboxPolicy::ReadOnly { .. } | SandboxPolicy::WorkspaceWrite { .. }
    ) {
        policy_sections.push(SEATBELT_PLATFORM_DEFAULTS.to_string());
    }

    let full_policy = policy_sections.join("\n");

    let dir_params = [
        file_read_dir_params,
        file_write_dir_params,
        macos_dir_params(),
    ]
    .concat();

    let mut args: Vec<String> = vec!["-p".to_string(), full_policy];
    let definition_args = dir_params
        .into_iter()
        .map(|(key, value)| format!("-D{key}={}", value.to_string_lossy()));
    args.extend(definition_args);
    args.push("--".to_string());
    args.extend(command);
    args
}

/// Spawn a child process under the macOS Seatbelt sandbox.
pub async fn spawn_command_under_seatbelt(
    command: Vec<String>,
    command_cwd: PathBuf,
    sandbox_policy: &SandboxPolicy,
    sandbox_policy_cwd: &Path,
    env: HashMap<String, String>,
) -> std::io::Result<tokio::process::Child> {
    let args = create_seatbelt_command_args(command, sandbox_policy, sandbox_policy_cwd);

    let mut cmd = tokio::process::Command::new(MACOS_PATH_TO_SEATBELT_EXECUTABLE);
    cmd.args(&args)
        .current_dir(&command_cwd)
        .envs(&env)
        .env("CODEX_SANDBOX", "seatbelt")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    cmd.spawn()
}

// ── Internal helpers ─────────────────────────────────────────────

fn build_write_policy(
    sandbox_policy: &SandboxPolicy,
    cwd: &Path,
) -> (String, Vec<(String, PathBuf)>) {
    if sandbox_policy.has_full_disk_write_access() {
        return (
            r#"(allow file-write* (regex #"^/"))"#.to_string(),
            Vec::new(),
        );
    }

    let writable_roots = sandbox_policy.get_writable_roots_with_cwd(cwd);
    if writable_roots.is_empty() {
        return (String::new(), Vec::new());
    }

    let mut policies = Vec::new();
    let mut params = Vec::new();

    for (index, wr) in writable_roots.iter().enumerate() {
        let canonical = wr
            .root
            .as_path()
            .canonicalize()
            .unwrap_or_else(|_| wr.root.to_path_buf());
        let root_param = format!("WRITABLE_ROOT_{index}");
        params.push((root_param.clone(), canonical));

        if wr.read_only_subpaths.is_empty() {
            policies.push(format!("(subpath (param \"{root_param}\"))"));
        } else {
            let mut parts = vec![format!("(subpath (param \"{root_param}\"))")];
            for (si, ro) in wr.read_only_subpaths.iter().enumerate() {
                let canonical_ro = ro.canonicalize().unwrap_or_else(|_| ro.to_path_buf());
                let ro_param = format!("WRITABLE_ROOT_{index}_RO_{si}");
                parts.push(format!("(require-not (subpath (param \"{ro_param}\")))"));
                params.push((ro_param, canonical_ro));
            }
            policies.push(format!("(require-all {} )", parts.join(" ")));
        }
    }

    let policy = format!("(allow file-write*\n{}\n)", policies.join(" "));
    (policy, params)
}

fn build_read_policy(
    sandbox_policy: &SandboxPolicy,
    cwd: &Path,
) -> (String, Vec<(String, PathBuf)>) {
    if sandbox_policy.has_full_disk_read_access() {
        return (
            "; allow read-only file operations\n(allow file-read*)".to_string(),
            Vec::new(),
        );
    }

    let readable_roots = sandbox_policy.get_readable_roots_with_cwd(cwd);
    if readable_roots.is_empty() {
        return (String::new(), Vec::new());
    }

    let mut policies = Vec::new();
    let mut params = Vec::new();

    for (index, root) in readable_roots.iter().enumerate() {
        let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let param = format!("READABLE_ROOT_{index}");
        params.push((param.clone(), canonical));
        policies.push(format!("(subpath (param \"{param}\"))"));
    }

    let policy = format!(
        "; allow read-only file operations\n(allow file-read*\n{}\n)",
        policies.join(" ")
    );
    (policy, params)
}

fn build_network_policy(sandbox_policy: &SandboxPolicy) -> String {
    if sandbox_policy.has_full_network_access() {
        format!("(allow network-outbound)\n(allow network-inbound)\n{SEATBELT_NETWORK_POLICY}")
    } else {
        String::new()
    }
}

/// Wraps libc::confstr to return a canonicalized PathBuf.
fn confstr_path(name: libc::c_int) -> Option<PathBuf> {
    let mut buf = vec![0_i8; (libc::PATH_MAX as usize) + 1];
    let len = unsafe { libc::confstr(name, buf.as_mut_ptr(), buf.len()) };
    if len == 0 {
        return None;
    }
    let cstr = unsafe { CStr::from_ptr(buf.as_ptr()) };
    let s = cstr.to_str().ok()?;
    let path = PathBuf::from(s);
    path.canonicalize().ok().or(Some(path))
}

fn macos_dir_params() -> Vec<(String, PathBuf)> {
    if let Some(p) = confstr_path(libc::_CS_DARWIN_USER_CACHE_DIR) {
        vec![("DARWIN_USER_CACHE_DIR".to_string(), p)]
    } else {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_policy_allows_cpu_sysctls() {
        assert!(SEATBELT_BASE_POLICY.contains("machdep.cpu.brand_string"));
        assert!(SEATBELT_BASE_POLICY.contains("hw.model"));
    }

    #[test]
    fn full_access_policy_allows_all_writes() {
        let (policy, params) =
            build_write_policy(&SandboxPolicy::DangerFullAccess, Path::new("/tmp"));
        assert!(policy.contains("(allow file-write*"));
        assert!(params.is_empty());
    }

    #[test]
    fn read_only_policy_allows_all_reads() {
        let (policy, _) =
            build_read_policy(&SandboxPolicy::new_read_only_policy(), Path::new("/tmp"));
        assert!(policy.contains("(allow file-read*)"));
    }

    #[test]
    fn network_policy_full_access() {
        let policy = build_network_policy(&SandboxPolicy::DangerFullAccess);
        assert!(policy.contains("(allow network-outbound)"));
        assert!(policy.contains("(allow network-inbound)"));
    }

    #[test]
    fn network_policy_no_access() {
        let policy = build_network_policy(&SandboxPolicy::new_read_only_policy());
        assert!(policy.is_empty());
    }

    #[test]
    fn seatbelt_args_structure() {
        let args = create_seatbelt_command_args(
            vec!["echo".into(), "hello".into()],
            &SandboxPolicy::new_read_only_policy(),
            Path::new("/tmp"),
        );
        assert_eq!(args[0], "-p");
        // Policy is args[1]
        assert!(args.contains(&"--".to_string()));
        assert!(args.contains(&"echo".to_string()));
        assert!(args.contains(&"hello".to_string()));
    }
}
