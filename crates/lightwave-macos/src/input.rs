//! Mouse and keyboard input simulation via enigo.

use anyhow::Result;

/// Click at a screen coordinate.
#[cfg(target_os = "macos")]
pub fn click(x: f64, y: f64) -> Result<()> {
    use enigo::{Coordinate, Enigo, Mouse, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to init input: {}", e))?;
    enigo
        .move_mouse(x as i32, y as i32, Coordinate::Abs)
        .map_err(|e| anyhow::anyhow!("Failed to move mouse: {}", e))?;
    enigo
        .button(enigo::Button::Left, enigo::Direction::Click)
        .map_err(|e| anyhow::anyhow!("Failed to click: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn click(_x: f64, _y: f64) -> Result<()> {
    anyhow::bail!("Input requires macOS")
}

/// Move mouse to a screen coordinate.
#[cfg(target_os = "macos")]
pub fn move_mouse(x: f64, y: f64) -> Result<()> {
    use enigo::{Coordinate, Enigo, Mouse, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to init input: {}", e))?;
    enigo
        .move_mouse(x as i32, y as i32, Coordinate::Abs)
        .map_err(|e| anyhow::anyhow!("Failed to move mouse: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn move_mouse(_x: f64, _y: f64) -> Result<()> {
    anyhow::bail!("Input requires macOS")
}

/// Type a string of text.
#[cfg(target_os = "macos")]
pub fn type_text(text: &str) -> Result<()> {
    use enigo::{Enigo, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to init input: {}", e))?;
    enigo
        .text(text)
        .map_err(|e| anyhow::anyhow!("Failed to type text: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn type_text(_text: &str) -> Result<()> {
    anyhow::bail!("Input requires macOS")
}

/// Press a key by name (e.g., "Return", "Escape", "Tab").
#[cfg(target_os = "macos")]
pub fn key_press(key_name: &str) -> Result<()> {
    use enigo::{Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to init input: {}", e))?;

    let key = match key_name.to_lowercase().as_str() {
        "return" | "enter" => Key::Return,
        "escape" | "esc" => Key::Escape,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "backspace" | "delete" => Key::Backspace,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        _ => anyhow::bail!("Unknown key: {}", key_name),
    };

    enigo
        .key(key, enigo::Direction::Click)
        .map_err(|e| anyhow::anyhow!("Failed to press key: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn key_press(_key_name: &str) -> Result<()> {
    anyhow::bail!("Input requires macOS")
}
