//! macOS Keychain integration.
//!
//! Read and write credentials natively using the macOS Keychain.
//! Eliminates the need for .env files or plaintext credential storage.

use anyhow::Result;

/// Service name prefix for all Augusta keychain items.
const SERVICE_PREFIX: &str = "com.lightwave.augusta";

/// Store a credential in the macOS Keychain.
pub fn store_credential(key: &str, value: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        store_credential_impl(key, value)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (key, value);
        anyhow::bail!("Keychain only available on macOS")
    }
}

/// Retrieve a credential from the macOS Keychain.
pub fn get_credential(key: &str) -> Result<Option<String>> {
    #[cfg(target_os = "macos")]
    {
        get_credential_impl(key)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = key;
        anyhow::bail!("Keychain only available on macOS")
    }
}

/// Delete a credential from the macOS Keychain.
pub fn delete_credential(key: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        delete_credential_impl(key)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = key;
        anyhow::bail!("Keychain only available on macOS")
    }
}

/// List all Augusta credentials in the Keychain (keys only, no values).
pub fn list_credentials() -> Result<Vec<String>> {
    #[cfg(target_os = "macos")]
    {
        list_credentials_impl()
    }
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("Keychain only available on macOS")
    }
}

// ── macOS implementations ────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn store_credential_impl(key: &str, value: &str) -> Result<()> {
    use security_framework::passwords::set_generic_password;

    let service = format!("{SERVICE_PREFIX}.{key}");
    set_generic_password(&service, key, value.as_bytes())
        .map_err(|e| anyhow::anyhow!("Keychain store failed for {key}: {e}"))?;

    tracing::debug!("Stored credential in Keychain: {key}");
    Ok(())
}

#[cfg(target_os = "macos")]
fn get_credential_impl(key: &str) -> Result<Option<String>> {
    use security_framework::passwords::get_generic_password;

    let service = format!("{SERVICE_PREFIX}.{key}");
    match get_generic_password(&service, key) {
        Ok(bytes) => {
            let value = String::from_utf8(bytes)
                .map_err(|e| anyhow::anyhow!("Keychain value for {key} is not valid UTF-8: {e}"))?;
            Ok(Some(value))
        }
        Err(e) => {
            // errSecItemNotFound = -25300
            if e.code() == -25300 {
                Ok(None)
            } else {
                Err(anyhow::anyhow!("Keychain read failed for {key}: {e}"))
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn delete_credential_impl(key: &str) -> Result<()> {
    use security_framework::passwords::delete_generic_password;

    let service = format!("{SERVICE_PREFIX}.{key}");
    match delete_generic_password(&service, key) {
        Ok(()) => {
            tracing::debug!("Deleted credential from Keychain: {key}");
            Ok(())
        }
        Err(e) => {
            if e.code() == -25300 {
                Ok(()) // Already gone
            } else {
                Err(anyhow::anyhow!("Keychain delete failed for {key}: {e}"))
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn list_credentials_impl() -> Result<Vec<String>> {
    // security-framework doesn't expose a direct search API for generic passwords.
    // Use `security find-generic-password` CLI as a fallback.
    use std::process::Command;

    let output = Command::new("security").args(["dump-keychain"]).output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut keys = Vec::new();

    for line in stdout.lines() {
        if line.contains(SERVICE_PREFIX) && line.contains("\"svce\"") {
            // Extract the service name after the prefix
            if let Some(start) = line.find(SERVICE_PREFIX) {
                let rest = &line[start + SERVICE_PREFIX.len()..];
                if let Some(dot_pos) = rest.find('.') {
                    if let Some(end) = rest[dot_pos + 1..].find('"') {
                        let key = &rest[dot_pos + 1..dot_pos + 1 + end];
                        if !key.is_empty() {
                            keys.push(key.to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(keys)
}
