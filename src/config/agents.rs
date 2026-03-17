use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// Agent preset identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPreset {
    Claude,
    Gemini,
    Codex,
    Cursor,
    Auggie,
    Amp,
    OpenCode,
    Copilot,
}

impl AgentPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::Auggie => "auggie",
            Self::Amp => "amp",
            Self::OpenCode => "opencode",
            Self::Copilot => "copilot",
        }
    }

    pub fn parse_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "gemini" => Some(Self::Gemini),
            "codex" => Some(Self::Codex),
            "cursor" => Some(Self::Cursor),
            "auggie" | "augusta" => Some(Self::Auggie),
            "amp" => Some(Self::Amp),
            "opencode" => Some(Self::OpenCode),
            "copilot" => Some(Self::Copilot),
            _ => None,
        }
    }
}

/// How an agent resumes a prior session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeStyle {
    /// `<command> --resume <session_id>` (Claude)
    Flag,
    /// `<command> resume <session_id>` (Codex)
    Subcommand,
}

/// Full description of an agent's behavior, startup, liveness, and hook support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPresetInfo {
    pub name: AgentPreset,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub process_names: Vec<String>,
    pub session_id_env: String,
    pub resume_flag: String,
    pub continue_flag: String,
    pub resume_style: ResumeStyle,
    pub supports_hooks: bool,
    pub supports_fork_session: bool,

    // Runtime config
    pub prompt_mode: String,
    pub instructions_file: String,
}

impl AgentPresetInfo {
    /// Build a command string for starting a new session.
    pub fn build_start_command(&self, extra_args: &[String]) -> String {
        let mut parts = vec![self.command.clone()];
        parts.extend(self.args.iter().cloned());
        parts.extend(extra_args.iter().cloned());
        parts.join(" ")
    }

    /// Build a command string for resuming a session.
    pub fn build_resume_command(&self, session_id: &str) -> String {
        let mut parts = vec![self.command.clone()];
        parts.extend(self.args.iter().cloned());
        match self.resume_style {
            ResumeStyle::Subcommand | ResumeStyle::Flag => {
                parts.push(self.resume_flag.clone());
                parts.push(session_id.to_string());
            }
        }
        parts.join(" ")
    }

    /// Build environment variables for this agent, merged with role env.
    pub fn build_env(&self, role_env: &HashMap<String, String>) -> HashMap<String, String> {
        let mut env = self.env.clone();
        env.extend(role_env.iter().map(|(k, v)| (k.clone(), v.clone())));
        env
    }
}

/// Create the built-in presets map.
fn builtin_presets() -> HashMap<AgentPreset, AgentPresetInfo> {
    let mut m = HashMap::new();

    m.insert(
        AgentPreset::Claude,
        AgentPresetInfo {
            name: AgentPreset::Claude,
            command: "claude".into(),
            args: vec![], // No permission bypass — hooks must fire
            env: HashMap::new(),
            process_names: vec!["node".into(), "claude".into()],
            session_id_env: "CLAUDE_SESSION_ID".into(),
            resume_flag: "--resume".into(),
            continue_flag: "--continue".into(),
            resume_style: ResumeStyle::Flag,
            supports_hooks: true,
            supports_fork_session: true,
            prompt_mode: "arg".into(),
            instructions_file: "AGENTS.md".into(),
        },
    );

    m.insert(
        AgentPreset::Codex,
        AgentPresetInfo {
            name: AgentPreset::Codex,
            command: "codex".into(),
            args: vec!["--dangerously-bypass-approvals-and-sandbox".into()],
            env: HashMap::new(),
            process_names: vec!["codex".into(), "node".into()],
            session_id_env: String::new(),
            resume_flag: "resume".into(),
            continue_flag: String::new(),
            resume_style: ResumeStyle::Subcommand,
            supports_hooks: false,
            supports_fork_session: false,
            prompt_mode: "arg".into(),
            instructions_file: "AGENTS.md".into(),
        },
    );

    m.insert(
        AgentPreset::OpenCode,
        AgentPresetInfo {
            name: AgentPreset::OpenCode,
            command: "opencode".into(),
            args: vec![],
            env: HashMap::from([("OPENCODE_PERMISSION".into(), r#"{"*":"allow"}"#.into())]),
            process_names: vec!["opencode".into(), "node".into(), "bun".into()],
            session_id_env: String::new(),
            resume_flag: String::new(),
            continue_flag: String::new(),
            resume_style: ResumeStyle::Flag,
            supports_hooks: false,
            supports_fork_session: false,
            prompt_mode: "arg".into(),
            instructions_file: String::new(),
        },
    );

    m
}

/// Thread-safe agent preset registry with built-in defaults + user overrides.
pub struct AgentRegistry {
    presets: RwLock<HashMap<AgentPreset, AgentPresetInfo>>,
}

impl AgentRegistry {
    /// Create a registry with built-in defaults.
    pub fn new() -> Self {
        Self {
            presets: RwLock::new(builtin_presets()),
        }
    }

    /// Get a preset by name.
    pub fn get(&self, preset: AgentPreset) -> Option<AgentPresetInfo> {
        self.presets.read().ok()?.get(&preset).cloned()
    }

    /// Get a preset by string name.
    pub fn get_by_name(&self, name: &str) -> Option<AgentPresetInfo> {
        let preset = AgentPreset::parse_name(name)?;
        self.get(preset)
    }

    /// Override or add a preset (from user config).
    pub fn set(&self, info: AgentPresetInfo) {
        if let Ok(mut presets) = self.presets.write() {
            presets.insert(info.name, info);
        }
    }

    /// List all available presets.
    pub fn list(&self) -> Vec<AgentPresetInfo> {
        self.presets
            .read()
            .ok()
            .map(|p| p.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Resolve process names for liveness detection with multi-level fallback.
    /// Priority: agent name match → command match → basename → fallback.
    pub fn resolve_process_names(&self, agent_name: &str) -> Vec<String> {
        if let Some(info) = self.get_by_name(agent_name) {
            if !info.process_names.is_empty() {
                return info.process_names;
            }
            return vec![info.command.clone()];
        }
        // Fallback: treat the agent name as the process name
        vec![agent_name.to_string()]
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_preset_has_no_permission_bypass() {
        let registry = AgentRegistry::new();
        let claude = registry.get(AgentPreset::Claude).unwrap();
        assert!(
            claude.args.is_empty(),
            "Claude args should be empty (no permission bypass)"
        );
        assert!(claude.supports_hooks, "Claude should support hooks");
    }

    #[test]
    fn build_resume_command() {
        let registry = AgentRegistry::new();
        let claude = registry.get(AgentPreset::Claude).unwrap();
        let cmd = claude.build_resume_command("sess-123");
        assert_eq!(cmd, "claude --resume sess-123");
    }

    #[test]
    fn resolve_unknown_agent_falls_back() {
        let registry = AgentRegistry::new();
        let names = registry.resolve_process_names("unknown-agent");
        assert_eq!(names, vec!["unknown-agent"]);
    }
}
