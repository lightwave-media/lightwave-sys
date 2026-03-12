//! Window management via CGWindowListCopyWindowInfo.
//!
//! Uses `osascript` for reliable cross-version compatibility rather than
//! low-level Core Foundation bindings which change between crate versions.

use crate::types::{Rect, WindowInfo};
use anyhow::Result;

/// List all on-screen windows.
#[cfg(target_os = "macos")]
pub fn list_windows() -> Result<Vec<WindowInfo>> {
    use std::process::Command;

    // Use System Events to get window information reliably
    let script = r#"
        set output to ""
        tell application "System Events"
            set procList to every process whose visible is true
            repeat with proc in procList
                set appName to name of proc
                set appPID to unix id of proc
                try
                    set winList to every window of proc
                    repeat with win in winList
                        set winName to name of win
                        try
                            set {x, y} to position of win
                            set {w, h} to size of win
                        on error
                            set {x, y} to {0, 0}
                            set {w, h} to {0, 0}
                        end try
                        set output to output & appName & "|||" & appPID & "|||" & winName & "|||" & x & "|||" & y & "|||" & w & "|||" & h & linefeed
                    end repeat
                end try
            end repeat
        end tell
        return output
    "#;

    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to list windows: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut windows = Vec::new();
    let mut id_counter: u32 = 1;

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split("|||").collect();
        if parts.len() >= 7 {
            let owner_name = parts[0].to_string();
            let owner_pid = parts[1].parse().unwrap_or(0);
            let title = parts[2].to_string();
            let x: f64 = parts[3].parse().unwrap_or(0.0);
            let y: f64 = parts[4].parse().unwrap_or(0.0);
            let w: f64 = parts[5].parse().unwrap_or(0.0);
            let h: f64 = parts[6].parse().unwrap_or(0.0);

            windows.push(WindowInfo {
                id: id_counter,
                title,
                owner_name,
                owner_pid,
                bounds: Rect {
                    x,
                    y,
                    width: w,
                    height: h,
                },
                on_screen: true,
                layer: 0,
            });
            id_counter += 1;
        }
    }

    Ok(windows)
}

#[cfg(not(target_os = "macos"))]
pub fn list_windows() -> Result<Vec<WindowInfo>> {
    Ok(Vec::new())
}

/// Focus a window by its owner PID (brings the app to front).
#[cfg(target_os = "macos")]
pub fn focus_window(pid: i32) -> Result<()> {
    use std::process::Command;

    let script = format!(
        "tell application \"System Events\" to set frontmost of (first process whose unix id is {}) to true",
        pid
    );
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to focus window: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to focus PID {}: {}", pid, stderr);
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn focus_window(_pid: i32) -> Result<()> {
    anyhow::bail!("Window focus requires macOS")
}
