//! Binary-level smoke tests — spawn `augusta` as a subprocess and verify output.
//!
//! These tests validate the compiled binary's CLI interface without
//! requiring a live LLM provider. They test subcommands that don't
//! need network access: version, memory stats, daemon status, help.

use std::process::Command;

fn augusta_bin() -> Command {
    // cargo sets this env var during `cargo test` to the compiled binary path
    let bin = env!("CARGO_BIN_EXE_augusta");
    Command::new(bin)
}

/// `augusta version` prints version, provider, model, memory, config path.
#[test]
fn binary_version_output() {
    let output = augusta_bin()
        .arg("version")
        .output()
        .expect("Failed to run augusta version");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("LightWave Augusta v"),
        "Should contain version string: {stdout}"
    );
    assert!(
        stdout.contains("Runtime: native"),
        "Should show runtime: {stdout}"
    );
    assert!(
        stdout.contains("Provider:"),
        "Should show provider: {stdout}"
    );
    assert!(stdout.contains("Model:"), "Should show model: {stdout}");
    assert!(stdout.contains("Memory:"), "Should show memory: {stdout}");
    assert!(stdout.contains("Config:"), "Should show config: {stdout}");
}

/// `augusta --help` prints usage information.
#[test]
fn binary_help_output() {
    let output = augusta_bin()
        .arg("--help")
        .output()
        .expect("Failed to run augusta --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Local AI agent runtime"),
        "Help should contain description: {stdout}"
    );
    assert!(
        stdout.contains("agent"),
        "Help should list agent subcommand: {stdout}"
    );
    assert!(
        stdout.contains("memory"),
        "Help should list memory subcommand: {stdout}"
    );
    assert!(
        stdout.contains("daemon"),
        "Help should list daemon subcommand: {stdout}"
    );
}

/// `augusta daemon status` works without a running daemon.
#[test]
fn binary_daemon_status_no_daemon() {
    let output = augusta_bin()
        .args(["daemon", "status"])
        .output()
        .expect("Failed to run augusta daemon status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("not running"),
        "Should report daemon not running: {stdout}"
    );
}

/// `augusta memory stats` works with default SQLite backend.
#[test]
fn binary_memory_stats() {
    let output = augusta_bin()
        .args(["memory", "stats"])
        .output()
        .expect("Failed to run augusta memory stats");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Memory Statistics"),
        "Should show memory stats header: {stdout}"
    );
    assert!(stdout.contains("Backend:"), "Should show backend: {stdout}");
    assert!(
        stdout.contains("Health:"),
        "Should show health status: {stdout}"
    );
}

/// `augusta channel list` shows available channels.
#[test]
fn binary_channel_list() {
    let output = augusta_bin()
        .args(["channel", "list"])
        .output()
        .expect("Failed to run augusta channel list");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cli"), "Should list CLI channel: {stdout}");
}

/// `augusta agent --help` shows agent subcommand usage.
#[test]
fn binary_agent_help() {
    let output = augusta_bin()
        .args(["agent", "--help"])
        .output()
        .expect("Failed to run augusta agent --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--message"),
        "Should show --message flag: {stdout}"
    );
    assert!(
        stdout.contains("--provider"),
        "Should show --provider flag: {stdout}"
    );
    assert!(
        stdout.contains("--model"),
        "Should show --model flag: {stdout}"
    );
}
