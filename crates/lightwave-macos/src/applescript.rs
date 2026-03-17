//! Native AppleScript execution.
//!
//! Execute AppleScript directly, faster than spawning osascript subprocess
//! for repeated calls. Falls back to osascript for initial implementation.

use anyhow::Result;

/// Execute an AppleScript and return the result as a string.
pub fn execute(script: &str) -> Result<String> {
    let output = std::process::Command::new("osascript")
        .args(["-e", script])
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("AppleScript failed: {}", stderr.trim())
    }
}

/// Execute an AppleScript from a file.
pub fn execute_file(path: &str) -> Result<String> {
    let output = std::process::Command::new("osascript").arg(path).output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("AppleScript file failed: {}", stderr.trim())
    }
}

/// Tell an application to perform an action.
pub fn tell_app(app_name: &str, action: &str) -> Result<String> {
    let script = format!(r#"tell application "{app_name}" to {action}"#);
    execute(&script)
}

/// Get a property from an application.
pub fn get_app_property(app_name: &str, property: &str) -> Result<String> {
    let script = format!(r#"tell application "{app_name}" to get {property}"#);
    execute(&script)
}

/// Check if an application is running.
pub fn is_app_running(app_name: &str) -> Result<bool> {
    let script =
        format!(r#"tell application "System Events" to (name of processes) contains "{app_name}""#);
    let result = execute(&script)?;
    Ok(result.to_lowercase() == "true")
}
