//! Screen capture and display info via xcap.

use crate::types::DisplayInfo;
use anyhow::Result;

/// List available displays.
#[cfg(target_os = "macos")]
pub fn list_displays() -> Result<Vec<DisplayInfo>> {
    use xcap::Monitor;

    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list displays: {}", e))?;
    let mut displays = Vec::new();

    for monitor in monitors.iter() {
        displays.push(DisplayInfo {
            id: monitor.id().unwrap_or(0),
            width: monitor.width().unwrap_or(0),
            height: monitor.height().unwrap_or(0),
            is_primary: monitor.is_primary().unwrap_or(false),
            scale_factor: monitor.scale_factor().unwrap_or(1.0) as f64,
        });
    }

    Ok(displays)
}

#[cfg(not(target_os = "macos"))]
pub fn list_displays() -> Result<Vec<DisplayInfo>> {
    Ok(Vec::new())
}

/// Capture the primary screen as a PNG-encoded byte buffer.
#[cfg(target_os = "macos")]
pub fn capture_screen() -> Result<Vec<u8>> {
    use xcap::Monitor;

    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list monitors: {}", e))?;
    let primary = monitors
        .into_iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| Monitor::all().ok()?.into_iter().next())
        .ok_or_else(|| anyhow::anyhow!("No display found"))?;

    let image = primary
        .capture_image()
        .map_err(|e| anyhow::anyhow!("Failed to capture screen: {}", e))?;

    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    image
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| anyhow::anyhow!("Failed to encode PNG: {}", e))?;

    Ok(buf)
}

#[cfg(not(target_os = "macos"))]
pub fn capture_screen() -> Result<Vec<u8>> {
    anyhow::bail!("Screen capture requires macOS")
}

/// Capture the screen and return as base64-encoded PNG.
pub fn capture_screen_base64() -> Result<String> {
    use base64::Engine;
    let png_bytes = capture_screen()?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&png_bytes))
}
