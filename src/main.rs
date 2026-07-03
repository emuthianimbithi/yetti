use anyhow::{Context, Result, anyhow};
use clap::Parser;
use tracing_subscriber::EnvFilter;

mod cli;
mod commands;
mod config;
mod database;
mod http;
mod monitoring;
mod notifications;
mod state;
mod transform;

#[tokio::main]
async fn main() -> Result<()> {
    let yetii = cli::Yetii::parse();
    initialize_tracing(yetii.verbose)?;

    if !matches!(
        yetii.commands,
        cli::Commands::Init { .. } | cli::Commands::CheckExistingOdbc
    ) {
        config::load_config_once(&yetii.file)
            .with_context(|| format!("failed to load configuration '{}'", yetii.file))?;
    }

    commands::going_through_commands(&yetii).await
}

fn initialize_tracing(verbose: bool) -> Result<()> {
    let default_level = if verbose { "debug" } else { "info" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .json()
        .try_init()
        .map_err(|error| anyhow!("failed to initialize tracing: {error}"))
}
