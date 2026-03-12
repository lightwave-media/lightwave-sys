//! macOS permission checks for accessibility and screen capture.

/// Check if the process has accessibility permissions (AXIsProcessTrusted).
#[cfg(target_os = "macos")]
pub fn is_accessibility_trusted() -> bool {
    extern "C" {
        fn AXIsProcessTrusted() -> u8;
    }
    unsafe { AXIsProcessTrusted() != 0 }
}

#[cfg(not(target_os = "macos"))]
pub fn is_accessibility_trusted() -> bool {
    false
}

/// Check if screen capture access is available.
#[cfg(target_os = "macos")]
pub fn has_screen_capture_access() -> bool {
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> u8;
    }
    unsafe { CGPreflightScreenCaptureAccess() != 0 }
}

#[cfg(not(target_os = "macos"))]
pub fn has_screen_capture_access() -> bool {
    false
}

/// Request screen capture access (shows system dialog if not granted).
#[cfg(target_os = "macos")]
pub fn request_screen_capture_access() -> bool {
    extern "C" {
        fn CGRequestScreenCaptureAccess() -> u8;
    }
    unsafe { CGRequestScreenCaptureAccess() != 0 }
}

#[cfg(not(target_os = "macos"))]
pub fn request_screen_capture_access() -> bool {
    false
}

/// Check all required permissions and return a status summary.
pub fn check_permissions() -> PermissionStatus {
    PermissionStatus {
        accessibility: is_accessibility_trusted(),
        screen_capture: has_screen_capture_access(),
    }
}

/// Permission status for macOS automation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PermissionStatus {
    pub accessibility: bool,
    pub screen_capture: bool,
}

impl PermissionStatus {
    pub fn all_granted(&self) -> bool {
        self.accessibility && self.screen_capture
    }

    pub fn missing_permissions(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.accessibility {
            missing.push("Accessibility (System Settings → Privacy & Security → Accessibility)");
        }
        if !self.screen_capture {
            missing.push(
                "Screen Recording (System Settings → Privacy & Security → Screen Recording)",
            );
        }
        missing
    }
}
