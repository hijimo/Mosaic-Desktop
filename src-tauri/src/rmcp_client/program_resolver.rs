use std::collections::HashMap;
use std::ffi::OsString;

#[cfg(unix)]
pub fn resolve(program: OsString, _env: &HashMap<String, String>) -> std::io::Result<OsString> {
    Ok(program)
}

#[cfg(windows)]
pub fn resolve(program: OsString, env: &HashMap<String, String>) -> std::io::Result<OsString> {
    let cwd = std::env::current_dir()
        .map_err(|e| std::io::Error::other(format!("Failed to get current directory: {e}")))?;
    let search_path = env.get("PATH");
    match which::which_in(&program, search_path, &cwd) {
        Ok(resolved) => Ok(resolved.into_os_string()),
        Err(_) => Ok(program),
    }
}
