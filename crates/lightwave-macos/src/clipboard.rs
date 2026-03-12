//! Clipboard access via NSPasteboard (through osascript).

use anyhow::Result;

/// Get the current clipboard text content.
#[cfg(target_os = "macos")]
pub fn get_clipboard() -> Result<String> {
    use std::process::Command;

    let output = Command::new("pbpaste")
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to read clipboard: {}", e))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn get_clipboard() -> Result<String> {
    anyhow::bail!("Clipboard requires macOS")
}

/// Set the clipboard text content.
#[cfg(target_os = "macos")]
pub fn set_clipboard(text: &str) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to write clipboard: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to write to pbcopy: {}", e))?;
    }

    child
        .wait()
        .map_err(|e| anyhow::anyhow!("pbcopy failed: {}", e))?;

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn set_clipboard(_text: &str) -> Result<()> {
    anyhow::bail!("Clipboard requires macOS")
}
