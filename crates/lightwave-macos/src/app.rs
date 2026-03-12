//! Application management via NSWorkspace / NSRunningApplication.

use crate::types::AppInfo;
use anyhow::Result;

/// List running applications.
#[cfg(target_os = "macos")]
pub fn list_apps() -> Result<Vec<AppInfo>> {
    use std::process::Command;

    // Use osascript to list running apps — avoids objc2 complexity
    let script = r#"
        set output to ""
        tell application "System Events"
            set appList to every process whose background only is false
            repeat with proc in appList
                set appName to name of proc
                set appPID to unix id of proc
                set isFront to (frontmost of proc) as string
                set isHidden to (visible of proc is false) as string
                set bundleID to ""
                try
                    set bundleID to bundle identifier of proc
                end try
                set output to output & appName & "|||" & bundleID & "|||" & appPID & "|||" & isFront & "|||" & isHidden & linefeed
            end repeat
        end tell
        return output
    "#;

    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to list apps: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut apps = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split("|||").collect();
        if parts.len() >= 5 {
            apps.push(AppInfo {
                name: parts[0].to_string(),
                bundle_id: if parts[1].is_empty() {
                    None
                } else {
                    Some(parts[1].to_string())
                },
                pid: parts[2].parse().unwrap_or(0),
                is_active: parts[3] == "true",
                is_hidden: parts[4] == "true",
            });
        }
    }

    Ok(apps)
}

#[cfg(not(target_os = "macos"))]
pub fn list_apps() -> Result<Vec<AppInfo>> {
    Ok(Vec::new())
}

/// Get the frontmost application.
#[cfg(target_os = "macos")]
pub fn frontmost_app() -> Result<Option<AppInfo>> {
    let apps = list_apps()?;
    Ok(apps.into_iter().find(|a| a.is_active))
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_app() -> Result<Option<AppInfo>> {
    Ok(None)
}

/// Launch an application by name or bundle ID.
#[cfg(target_os = "macos")]
pub fn launch_app(name_or_bundle_id: &str) -> Result<()> {
    use std::process::Command;

    // Try bundle ID first, fall back to name
    let script = format!(
        r#"
        try
            tell application id "{0}" to activate
        on error
            tell application "{0}" to activate
        end try
        "#,
        name_or_bundle_id
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to launch app: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to launch '{}': {}", name_or_bundle_id, stderr);
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn launch_app(_name_or_bundle_id: &str) -> Result<()> {
    anyhow::bail!("App launch requires macOS")
}

/// Quit an application by name.
#[cfg(target_os = "macos")]
pub fn quit_app(name: &str) -> Result<()> {
    use std::process::Command;

    let script = format!(r#"tell application "{}" to quit"#, name);
    Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to quit app: {}", e))?;

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn quit_app(_name: &str) -> Result<()> {
    anyhow::bail!("App quit requires macOS")
}
