//! Channel subsystem — messaging platform adapters.
//!
//! Each channel implements the [`Channel`] trait defined in [`traits`].
//! Augusta ships with the CLI channel (stdin/stdout) and the orchestrator
//! channel (Redis Streams for Elixir orchestrator integration).

pub mod cli;
pub mod orchestrator;
pub mod traits;

pub use cli::CliChannel;
pub use orchestrator::OrchestratorChannel;
pub use traits::{Channel, ChannelMessage, SendMessage};

use crate::agent::loop_::{build_tool_instructions, run_tool_call_loop, scrub_credentials};
use crate::approval::ApprovalManager;
use crate::config::Config;
use crate::memory::{self, Memory};
use crate::observability;
use crate::providers::{self, ChatMessage, Provider};
use crate::runtime;
use crate::security::SecurityPolicy;
use crate::tools;
use anyhow::Result;
use std::sync::Arc;
use tracing::info;

/// Resolve which provider to use (config default or fallback)
fn resolved_default_provider(config: &Config) -> String {
    config
        .default_provider
        .clone()
        .unwrap_or_else(|| "anthropic".to_string())
}

/// Resolve which model to use
fn resolved_default_model(config: &Config) -> String {
    config
        .default_model
        .clone()
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string())
}

/// Start the orchestrator channel — listens on Redis Streams for tasks
/// from the Elixir orchestrator. Tasks are processed concurrently via
/// tokio::spawn, so multiple tasks can run in parallel.
pub async fn start_orchestrator(config: Config) -> Result<()> {
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

    let provider_name = resolved_default_provider(&config);
    let provider_runtime_options = providers::ProviderRuntimeOptions {
        auth_profile_override: None,
        provider_api_url: config.api_url.clone(),
        augusta_dir: config.config_path.parent().map(std::path::PathBuf::from),
        secrets_encrypt: config.secrets.encrypt,
        reasoning_enabled: config.runtime.reasoning_enabled,
        provider_timeout_secs: Some(config.provider_timeout_secs),
    };
    let provider: Arc<dyn Provider> = Arc::from(providers::create_resilient_provider_with_options(
        &provider_name,
        config.api_key.as_deref(),
        config.api_url.as_deref(),
        &config.reliability,
        &provider_runtime_options,
    )?);

    if let Err(e) = provider.warmup().await {
        tracing::warn!("Provider warmup failed (non-fatal): {e}");
    }

    let runtime: Arc<dyn runtime::RuntimeAdapter> =
        Arc::from(runtime::create_runtime(&config.runtime)?);
    let security = Arc::new(SecurityPolicy::from_config(
        &config.autonomy,
        &config.workspace_dir,
    ));
    let model = Arc::new(resolved_default_model(&config));
    let temperature = config.default_temperature;
    let mem: Arc<dyn Memory> = Arc::from(memory::create_memory(
        &config.memory,
        &config.workspace_dir,
        config.api_key.as_deref(),
    )?);

    let workspace = config.workspace_dir.clone();
    let tools_registry = Arc::new(tools::all_tools_with_runtime(
        Arc::new(config.clone()),
        &security,
        runtime,
        Arc::clone(&mem),
        None,
        None,
        &config.browser,
        &config.http_request,
        &config.web_fetch,
        &workspace,
        &config.agents,
        config.api_key.as_deref(),
        &config,
    ));

    let native_tools = provider.supports_native_tools();
    let system_prompt = {
        let mut sp = "You are LightWave Augusta, a local AI agent running on macOS.\n\
             You have access to shell, file, memory, browser, and desktop automation tools.\n\
             Be concise and direct. Execute tasks autonomously when possible."
            .to_string();
        if !native_tools {
            sp.push_str(&build_tool_instructions(tools_registry.as_ref()));
        }
        Arc::new(sp)
    };

    info!("Starting orchestrator channel");
    info!("  Redis:    {redis_url}");
    info!("  Model:    {model}");
    info!("  Provider: {provider_name}");
    info!("  Tools:    {} registered", tools_registry.len());

    let orch_channel = OrchestratorChannel::new(redis_url, None, None);

    // Start heartbeat
    #[cfg(feature = "orchestrator")]
    orch_channel.start_heartbeat(tools_registry.len());

    let channel: Arc<dyn Channel> = Arc::new(orch_channel);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ChannelMessage>(64);

    // Spawn orchestrator listener
    let channel_clone = Arc::clone(&channel);
    tokio::spawn(async move {
        if let Err(e) = channel_clone.listen(tx).await {
            tracing::error!("Orchestrator channel error: {e}");
        }
    });

    let provider_name = Arc::new(provider_name);
    let observer = Arc::new(observability::create_observer(&config.observability));
    let approval = Arc::new(ApprovalManager::from_config(&config.autonomy));
    let multimodal = Arc::new(config.multimodal.clone());
    let max_tool_iterations = config.agent.max_tool_iterations;
    let dedup_exempt = Arc::new(config.agent.tool_call_dedup_exempt.clone());

    // Process messages concurrently — each task is spawned independently
    while let Some(msg) = rx.recv().await {
        let provider = Arc::clone(&provider);
        let tools_registry = Arc::clone(&tools_registry);
        let observer = Arc::clone(&observer);
        let provider_name = Arc::clone(&provider_name);
        let model = Arc::clone(&model);
        let system_prompt = Arc::clone(&system_prompt);
        let channel = Arc::clone(&channel);
        let approval = Arc::clone(&approval);
        let multimodal = Arc::clone(&multimodal);
        let dedup_exempt = Arc::clone(&dedup_exempt);

        tokio::spawn(async move {
            let start = std::time::Instant::now();
            info!(run_id = %msg.id, sender = %msg.sender, "Processing orchestrator task");

            let mut history = vec![
                ChatMessage::system(&*system_prompt),
                ChatMessage::user(&msg.content),
            ];

            let result = run_tool_call_loop(
                provider.as_ref(),
                &mut history,
                tools_registry.as_ref(),
                observer.as_ref(),
                &provider_name,
                &model,
                temperature,
                false,
                Some(approval.as_ref()),
                "orchestrator",
                &multimodal,
                max_tool_iterations,
                None,
                None,
                None,
                &[],
                &dedup_exempt,
            )
            .await;

            let duration_ms = start.elapsed().as_millis() as u64;

            let output = match result {
                Ok(response) => scrub_credentials(&response),
                Err(e) => format!("Error: {e}"),
            };

            // Append duration marker so send() can extract it
            let tagged = format!("__duration_ms:{duration_ms}__\n{output}");
            if let Err(e) = channel
                .send(&SendMessage::new(&tagged, &msg.reply_target))
                .await
            {
                tracing::error!(run_id = %msg.id, error = %e, "Failed to publish result");
            }

            info!(run_id = %msg.id, duration_ms = duration_ms, "Task complete");
        });
    }

    Ok(())
}

/// Start the CLI channel loop — the primary local interaction mode.
pub async fn start_cli(config: Config) -> Result<()> {
    let provider_name = resolved_default_provider(&config);
    let provider_runtime_options = providers::ProviderRuntimeOptions {
        auth_profile_override: None,
        provider_api_url: config.api_url.clone(),
        augusta_dir: config.config_path.parent().map(std::path::PathBuf::from),
        secrets_encrypt: config.secrets.encrypt,
        reasoning_enabled: config.runtime.reasoning_enabled,
        provider_timeout_secs: Some(config.provider_timeout_secs),
    };
    let provider: Arc<dyn Provider> = Arc::from(providers::create_resilient_provider_with_options(
        &provider_name,
        config.api_key.as_deref(),
        config.api_url.as_deref(),
        &config.reliability,
        &provider_runtime_options,
    )?);

    if let Err(e) = provider.warmup().await {
        tracing::warn!("Provider warmup failed (non-fatal): {e}");
    }

    let runtime: Arc<dyn runtime::RuntimeAdapter> =
        Arc::from(runtime::create_runtime(&config.runtime)?);
    let security = Arc::new(SecurityPolicy::from_config(
        &config.autonomy,
        &config.workspace_dir,
    ));
    let model = resolved_default_model(&config);
    let temperature = config.default_temperature;
    let mem: Arc<dyn Memory> = Arc::from(memory::create_memory(
        &config.memory,
        &config.workspace_dir,
        config.api_key.as_deref(),
    )?);

    let workspace = config.workspace_dir.clone();
    let tools_registry = Arc::new(tools::all_tools_with_runtime(
        Arc::new(config.clone()),
        &security,
        runtime,
        Arc::clone(&mem),
        None,
        None,
        &config.browser,
        &config.http_request,
        &config.web_fetch,
        &workspace,
        &config.agents,
        config.api_key.as_deref(),
        &config,
    ));

    let native_tools = provider.supports_native_tools();
    let mut system_prompt = "You are LightWave Augusta, a local AI agent running on macOS.\n\
         You have access to shell, file, memory, browser, and desktop automation tools.\n\
         Be concise and direct. Execute tasks autonomously when possible."
        .to_string();
    if !native_tools {
        system_prompt.push_str(&build_tool_instructions(tools_registry.as_ref()));
    }

    println!("🦀 LightWave Augusta");
    println!("  Model:    {model}");
    println!("  Provider: {provider_name}");
    println!("  Tools:    {} registered", tools_registry.len());
    println!();

    let channel: Arc<dyn Channel> = Arc::new(CliChannel::new());
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ChannelMessage>(64);

    // Spawn CLI listener
    let channel_clone = Arc::clone(&channel);
    tokio::spawn(async move {
        if let Err(e) = channel_clone.listen(tx).await {
            tracing::error!("CLI channel error: {e}");
        }
    });

    let observer = observability::create_observer(&config.observability);
    let approval = ApprovalManager::from_config(&config.autonomy);

    // Process messages
    while let Some(msg) = rx.recv().await {
        info!("Processing message from {}", msg.sender);

        let mut history = vec![
            ChatMessage::system(&system_prompt),
            ChatMessage::user(&msg.content),
        ];

        let result = run_tool_call_loop(
            provider.as_ref(),
            &mut history,
            tools_registry.as_ref(),
            &observer,
            &provider_name,
            &model,
            temperature,
            false,
            Some(&approval),
            "cli",
            &config.multimodal,
            config.agent.max_tool_iterations,
            None,
            None,
            None,
            &[],
            &config.agent.tool_call_dedup_exempt,
        )
        .await;

        match result {
            Ok(response) => {
                let clean = scrub_credentials(&response);
                channel
                    .send(&SendMessage::new(&clean, &msg.reply_target))
                    .await?;
            }
            Err(e) => {
                let error_msg = format!("Error: {e}");
                channel
                    .send(&SendMessage::new(&error_msg, &msg.reply_target))
                    .await?;
            }
        }
    }

    Ok(())
}
