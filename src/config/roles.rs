use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// Scope of a role: town-level (one per town) or rig-level (one+ per rig).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleScope {
    Town,
    Rig,
}

/// Session configuration for a role.
#[derive(Debug, Clone, Deserialize)]
pub struct RoleSessionConfig {
    /// tmux session name pattern. Supports: {prefix}, {name}, {town}, {rig}
    pub pattern: String,
    /// Working directory pattern. Same placeholders.
    pub work_dir: String,
    /// Whether workspace needs git sync before starting.
    pub needs_pre_sync: bool,
    /// Command to run in the session. Default: "exec claude"
    pub start_command: Option<String>,
}

/// Health check thresholds for a role.
#[derive(Debug, Clone, Deserialize)]
pub struct RoleHealthConfig {
    /// How long to wait for a health check response.
    pub ping_timeout: String,
    /// How many consecutive failures before force-kill.
    pub consecutive_failures: u32,
    /// Cooldown after kill before restarting.
    pub kill_cooldown: String,
    /// Time before marking as stuck.
    pub stuck_threshold: String,
}

impl RoleHealthConfig {
    pub fn ping_timeout_duration(&self) -> Duration {
        parse_duration(&self.ping_timeout).unwrap_or(Duration::from_secs(30))
    }

    pub fn kill_cooldown_duration(&self) -> Duration {
        parse_duration(&self.kill_cooldown).unwrap_or(Duration::from_secs(300))
    }

    pub fn stuck_threshold_duration(&self) -> Duration {
        parse_duration(&self.stuck_threshold).unwrap_or(Duration::from_secs(3600))
    }
}

/// Full role definition loaded from TOML.
#[derive(Debug, Clone, Deserialize)]
pub struct RoleDefinition {
    pub role: String,
    pub scope: RoleScope,
    pub nudge: String,
    pub prompt_template: String,
    pub session: RoleSessionConfig,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub health: RoleHealthConfig,
}

impl RoleDefinition {
    pub fn start_command(&self) -> &str {
        self.session
            .start_command
            .as_deref()
            .unwrap_or("exec claude")
    }

    /// Expand template variables in session pattern.
    pub fn expand_session_pattern(&self, vars: &HashMap<String, String>) -> String {
        expand_template(&self.session.pattern, vars)
    }

    /// Expand template variables in work_dir.
    pub fn expand_work_dir(&self, vars: &HashMap<String, String>) -> String {
        expand_template(&self.session.work_dir, vars)
    }

    /// Expand template variables in env values.
    pub fn expand_env(&self, vars: &HashMap<String, String>) -> HashMap<String, String> {
        self.env
            .iter()
            .map(|(k, v)| (k.clone(), expand_template(v, vars)))
            .collect()
    }
}

/// Load all role definitions from a directory of TOML files.
pub fn load_roles(dir: &Path) -> Result<HashMap<String, RoleDefinition>> {
    let mut roles = HashMap::new();

    if !dir.exists() {
        return Ok(roles);
    }

    for entry in std::fs::read_dir(dir).context("reading roles directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "toml") {
            continue;
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let role: RoleDefinition =
            toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
        roles.insert(role.role.clone(), role);
    }

    Ok(roles)
}

/// Expand `{key}` placeholders in a template string.
fn expand_template(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{key}}}"), value);
    }
    result
}

/// Parse a duration string like "30s", "5m", "1h", "4h".
fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_str, unit) = s.split_at(s.len().saturating_sub(1));
    let num: u64 = num_str.parse().ok()?;
    match unit {
        "s" => Some(Duration::from_secs(num)),
        "m" => Some(Duration::from_secs(num * 60)),
        "h" => Some(Duration::from_secs(num * 3600)),
        "d" => Some(Duration::from_secs(num * 86400)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_works() {
        assert_eq!(parse_duration("30s"), Some(Duration::from_secs(30)));
        assert_eq!(parse_duration("5m"), Some(Duration::from_secs(300)));
        assert_eq!(parse_duration("1h"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_duration("4h"), Some(Duration::from_secs(14400)));
    }

    #[test]
    fn expand_template_works() {
        let vars = HashMap::from([
            ("prefix".into(), "dev".into()),
            ("name".into(), "alpha".into()),
            ("rig".into(), "rig1".into()),
        ]);
        assert_eq!(
            expand_template("{prefix}-crew-{name}", &vars),
            "dev-crew-alpha"
        );
    }

    #[test]
    fn parse_crew_toml() {
        let toml_str = r#"
role = "crew"
scope = "rig"
nudge = "Check your hook."
prompt_template = "crew.md.tmpl"

[session]
pattern = "{prefix}-crew-{name}"
work_dir = "{town}/{rig}/crew/{name}"
needs_pre_sync = true
start_command = "exec claude"

[env]
GT_ROLE = "{rig}/crew/{name}"
GT_SCOPE = "rig"

[health]
ping_timeout = "30s"
consecutive_failures = 3
kill_cooldown = "5m"
stuck_threshold = "4h"
"#;
        let role: RoleDefinition = toml::from_str(toml_str).unwrap();
        assert_eq!(role.role, "crew");
        assert_eq!(role.scope, RoleScope::Rig);
        assert_eq!(role.health.consecutive_failures, 3);
        assert_eq!(role.start_command(), "exec claude");
    }
}
