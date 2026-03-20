//! Event bus for feeding events to the TUI and persisting to disk.
//!
//! Events are written as JSONL to `~/Library/Logs/Augusta/feed.jsonl`.
//! The feed TUI reads this file on startup and tails it for new events.

use super::feed::{FeedApp, FeedEvent, FeedEventType};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Serializable event record for JSONL persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub timestamp: String,
    pub agent: String,
    pub role: String,
    pub event_type: String,
    pub message: String,
}

impl EventRecord {
    /// Create a new event record with current UTC timestamp.
    pub fn new(agent: &str, role: &str, event_type: &str, message: &str) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            agent: agent.to_string(),
            role: role.to_string(),
            event_type: event_type.to_string(),
            message: message.to_string(),
        }
    }

    /// Convert to a FeedEvent for the TUI.
    pub fn to_feed_event(&self) -> FeedEvent {
        FeedEvent {
            timestamp: Instant::now(),
            agent: self.agent.clone(),
            role: self.role.clone(),
            event_type: parse_event_type(&self.event_type),
            message: self.message.clone(),
        }
    }
}

fn parse_event_type(s: &str) -> FeedEventType {
    match s {
        "agent_started" => FeedEventType::AgentStarted,
        "agent_stopped" => FeedEventType::AgentStopped,
        "agent_stuck" => FeedEventType::AgentStuck,
        "agent_killed" => FeedEventType::AgentKilled,
        "task_assigned" => FeedEventType::TaskAssigned,
        "task_completed" => FeedEventType::TaskCompleted,
        "task_failed" => FeedEventType::TaskFailed,
        "ping_success" => FeedEventType::PingSuccess,
        "ping_failed" => FeedEventType::PingFailed,
        "nudge" => FeedEventType::Nudge,
        "handoff" => FeedEventType::Handoff,
        other => FeedEventType::Custom(other.to_string()),
    }
}

/// Default path for the event log.
pub fn default_event_log_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join("Library/Logs/Augusta/feed.jsonl")
}

/// Append an event to the JSONL log file.
pub fn append_event(path: &Path, record: &EventRecord) -> anyhow::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let line = serde_json::to_string(record)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

/// Emit an event to the default log path.
pub fn emit(agent: &str, role: &str, event_type: &str, message: &str) {
    let record = EventRecord::new(agent, role, event_type, message);
    let path = default_event_log_path();
    if let Err(e) = append_event(&path, &record) {
        tracing::warn!("Failed to write feed event: {e}");
    }
}

/// Load events from a JSONL file into a FeedApp.
pub fn load_events(path: &Path, app: &mut FeedApp) -> anyhow::Result<usize> {
    use std::io::BufRead;

    if !path.exists() {
        return Ok(0);
    }

    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut count = 0;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<EventRecord>(&line) {
            Ok(record) => {
                app.push_event(record.to_feed_event());
                count += 1;
            }
            Err(e) => {
                tracing::debug!("Skipping malformed feed event: {e}");
            }
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn event_record_roundtrip() {
        let record = EventRecord::new("agent-1", "crew", "agent_started", "Started processing");
        let json = serde_json::to_string(&record).unwrap();
        let parsed: EventRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent, "agent-1");
        assert_eq!(parsed.event_type, "agent_started");
    }

    #[test]
    fn parse_event_types() {
        assert!(matches!(
            parse_event_type("agent_started"),
            FeedEventType::AgentStarted
        ));
        assert!(matches!(
            parse_event_type("task_failed"),
            FeedEventType::TaskFailed
        ));
        assert!(matches!(
            parse_event_type("unknown"),
            FeedEventType::Custom(_)
        ));
    }

    #[test]
    fn append_and_load_events() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path();

        let r1 = EventRecord::new("a1", "crew", "agent_started", "Started");
        let r2 = EventRecord::new("a2", "crew", "task_completed", "Done");
        append_event(path, &r1).unwrap();
        append_event(path, &r2).unwrap();

        let mut app = FeedApp::new(100);
        let count = load_events(path, &mut app).unwrap();
        assert_eq!(count, 2);
        assert_eq!(app.events.len(), 2);
    }

    #[test]
    fn load_missing_file_returns_zero() {
        let mut app = FeedApp::new(100);
        let count = load_events(Path::new("/nonexistent/path.jsonl"), &mut app).unwrap();
        assert_eq!(count, 0);
    }
}
