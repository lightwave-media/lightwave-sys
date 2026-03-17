//! macOS Accessibility API (AXUIElement) queries.
//!
//! Inspect UI element trees, find buttons/fields, read screen content without screenshots.
//! Requires Accessibility permission in System Settings > Privacy & Security.

use anyhow::Result;
use serde::Serialize;

/// A simplified view of an AXUIElement.
#[derive(Debug, Clone, Serialize)]
pub struct UIElement {
    pub role: String,
    pub title: Option<String>,
    pub value: Option<String>,
    pub description: Option<String>,
    pub position: Option<(f64, f64)>,
    pub size: Option<(f64, f64)>,
    pub children_count: usize,
    pub is_focused: bool,
    pub is_enabled: bool,
}

/// Query the accessibility tree of the frontmost application.
pub fn query_frontmost_app() -> Result<Vec<UIElement>> {
    #[cfg(target_os = "macos")]
    {
        query_frontmost_app_impl()
    }
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("Accessibility API only available on macOS")
    }
}

/// Find UI elements matching a role (e.g., "AXButton", "AXTextField").
pub fn find_elements_by_role(role: &str) -> Result<Vec<UIElement>> {
    #[cfg(target_os = "macos")]
    {
        find_elements_by_role_impl(role)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = role;
        anyhow::bail!("Accessibility API only available on macOS")
    }
}

/// Find UI elements containing specific text in title or value.
pub fn find_elements_by_text(text: &str) -> Result<Vec<UIElement>> {
    #[cfg(target_os = "macos")]
    {
        find_elements_by_text_impl(text)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
        anyhow::bail!("Accessibility API only available on macOS")
    }
}

// ── macOS implementations via Core Foundation FFI ────────────────────────────

#[cfg(target_os = "macos")]
fn query_frontmost_app_impl() -> Result<Vec<UIElement>> {
    use std::process::Command;

    // Use osascript to get frontmost app's UI elements via System Events.
    // This is the reliable approach until we add objc2 for direct AXUIElement access.
    let output = Command::new("osascript")
        .args([
            "-e",
            r#"
            tell application "System Events"
                set frontApp to first application process whose frontmost is true
                set appName to name of frontApp
                set winCount to count of windows of frontApp
                set result to "app:" & appName & "|windows:" & winCount
                if winCount > 0 then
                    set win to window 1 of frontApp
                    set groups to count of groups of win
                    set buttons to count of buttons of win
                    set fields to count of text fields of win
                    set result to result & "|groups:" & groups & "|buttons:" & buttons & "|fields:" & fields
                end if
                return result
            end tell
            "#,
        ])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split('|').collect();

    let mut elements = Vec::new();
    for part in &parts {
        if let Some((key, value)) = part.split_once(':') {
            elements.push(UIElement {
                role: format!("AX{}", capitalize(key)),
                title: Some(value.to_string()),
                value: None,
                description: None,
                position: None,
                size: None,
                children_count: 0,
                is_focused: false,
                is_enabled: true,
            });
        }
    }

    Ok(elements)
}

#[cfg(target_os = "macos")]
fn find_elements_by_role_impl(role: &str) -> Result<Vec<UIElement>> {
    use std::process::Command;

    let script = format!(
        r#"
        tell application "System Events"
            set frontApp to first application process whose frontmost is true
            if (count of windows of frontApp) > 0 then
                set win to window 1 of frontApp
                set matched to {{}}
                try
                    set matched to every {role} of win
                end try
                set result to ""
                repeat with elem in matched
                    try
                        set elemName to name of elem
                        set result to result & elemName & "|"
                    end try
                end repeat
                return result
            end if
            return ""
        end tell
        "#,
        role = role.replace("AX", "").to_lowercase()
    );

    let output = Command::new("osascript").args(["-e", &script]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let elements: Vec<UIElement> = stdout
        .trim()
        .split('|')
        .filter(|s| !s.is_empty())
        .map(|name| UIElement {
            role: role.to_string(),
            title: Some(name.to_string()),
            value: None,
            description: None,
            position: None,
            size: None,
            children_count: 0,
            is_focused: false,
            is_enabled: true,
        })
        .collect();

    Ok(elements)
}

#[cfg(target_os = "macos")]
fn find_elements_by_text_impl(text: &str) -> Result<Vec<UIElement>> {
    use std::process::Command;

    let script = format!(
        r#"
        tell application "System Events"
            set frontApp to first application process whose frontmost is true
            if (count of windows of frontApp) > 0 then
                set win to window 1 of frontApp
                set matched to {{}}
                try
                    set matched to every UI element of win whose name contains "{text}"
                end try
                set result to ""
                repeat with elem in matched
                    try
                        set elemRole to role of elem
                        set elemName to name of elem
                        set result to result & elemRole & ":" & elemName & "|"
                    end try
                end repeat
                return result
            end if
            return ""
        end tell
        "#,
        text = text.replace('"', r#"\""#)
    );

    let output = Command::new("osascript").args(["-e", &script]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let elements: Vec<UIElement> = stdout
        .trim()
        .split('|')
        .filter(|s| !s.is_empty())
        .filter_map(|entry| {
            let (role, name) = entry.split_once(':')?;
            Some(UIElement {
                role: role.to_string(),
                title: Some(name.to_string()),
                value: None,
                description: None,
                position: None,
                size: None,
                children_count: 0,
                is_focused: false,
                is_enabled: true,
            })
        })
        .collect();

    Ok(elements)
}

#[cfg(target_os = "macos")]
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}
