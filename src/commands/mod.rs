mod daemon;
mod initialize;
mod odbc;
mod run;
mod setup;

use crate::cli::{Commands, DaemonCommand, Yetii};
use crate::config;
use anyhow::{Result, bail};

pub async fn going_through_commands(yetii: &Yetii) -> Result<()> {
    match &yetii.commands {
        Commands::Init { path } => {
            let config_name = std::path::Path::new(&yetii.file)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("yetii.yaml");
            let message = initialize::initialize_yetii_config(config_name, path)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("{message}");
        }
        Commands::CheckExistingOdbc => {
            let output = tokio::task::spawn_blocking(odbc::check_odbc_drivers).await??;
            println!("ODBC configuration:\n{output}");
        }
        Commands::Setup { dry_run } => {
            let config = config::get_config()?.clone();
            let report = setup::run(&config.databases, *dry_run).await?;
            println!("{report}");
        }
        Commands::Run { query, force } => {
            let report = run::run(query.as_deref(), *force).await?;
            println!("{report}");
            if !report.failures.is_empty() {
                for failure in &report.failures {
                    tracing::error!(
                        query = %failure.query,
                        error = %failure.error,
                        "query failed"
                    );
                }
                bail!("{} query execution(s) failed", report.failures.len());
            }
        }
        Commands::CheckConfig => {
            let config = config::get_config()?;
            config.validate()?;
            tracing::info!("configuration is valid");
        }
        Commands::Daemon { command } => match command {
            DaemonCommand::Start {
                detach,
                pid_file,
                log_file,
            } => {
                let message = daemon::start(yetii, *detach, pid_file, log_file).await?;
                println!("{message}");
            }
            DaemonCommand::Status { pid_file } => {
                let message = daemon::status(pid_file)?;
                println!("{message}");
            }
            DaemonCommand::Stop { pid_file } => {
                let message = daemon::stop(pid_file)?;
                println!("{message}");
            }
        },
    }
    Ok(())
}
