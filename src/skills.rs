//! Skills stub module for Augusta.
//! Skills loading from ZeroClaw community registry was stripped.
//! Augusta skills will be loaded from the workspace directory.

use crate::config::{Config, SkillsPromptInjectionMode};
use std::collections::HashMap;
use std::path::Path;

/// A skill definition.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub tools: Vec<SkillTool>,
    pub prompts: Vec<String>,
    pub location: Option<String>,
}

/// A tool defined within a skill.
#[derive(Debug, Clone)]
pub struct SkillTool {
    pub name: String,
    pub description: String,
    pub kind: String,
    pub command: String,
    pub args: HashMap<String, String>,
}

/// Load skills from workspace directory and config.
pub fn load_skills_with_config(_workspace: &Path, _config: &Config) -> Vec<Skill> {
    // TODO: implement workspace skill loading for Augusta
    Vec::new()
}

/// Escape XML special characters.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Convert skills to a prompt section for the LLM.
pub fn skills_to_prompt_with_mode(
    skills: &[Skill],
    workspace: &Path,
    mode: SkillsPromptInjectionMode,
) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let compact = matches!(mode, SkillsPromptInjectionMode::Compact);
    let mut out = String::from("<available_skills>\n");

    for skill in skills {
        out.push_str("<skill>\n");
        out.push_str(&format!("<name>{}</name>\n", xml_escape(&skill.name)));
        out.push_str(&format!(
            "<description>{}</description>\n",
            xml_escape(&skill.description)
        ));

        if let Some(ref loc) = skill.location {
            // Make location relative to workspace
            let rel = Path::new(loc)
                .strip_prefix(workspace)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| loc.clone());
            out.push_str(&format!("<location>{}</location>\n", xml_escape(&rel)));
        }

        if !compact {
            for prompt in &skill.prompts {
                out.push_str(&format!(
                    "<instruction>{}</instruction>\n",
                    xml_escape(prompt)
                ));
            }
            if !skill.tools.is_empty() {
                out.push_str("<tools>\n");
                for tool in &skill.tools {
                    out.push_str("<tool>\n");
                    out.push_str(&format!("<name>{}</name>\n", xml_escape(&tool.name)));
                    out.push_str(&format!(
                        "<description>{}</description>\n",
                        xml_escape(&tool.description)
                    ));
                    out.push_str(&format!("<kind>{}</kind>\n", xml_escape(&tool.kind)));
                    out.push_str("</tool>\n");
                }
                out.push_str("</tools>\n");
            }
        }

        out.push_str("</skill>\n");
    }

    out.push_str("</available_skills>");
    out
}
