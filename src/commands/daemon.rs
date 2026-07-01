use super::run;
use crate::cli::Yetii;
use crate::config;
use crate::config::execution_config::SchedulerConfig;
use crate::config::query_config::QueryConfig;
use crate::config::schedule_config::normalized_cron;
use anyhow::{Context, Result, bail};
use std::fs::{OpenOptions, read_to_string, remove_file};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio_cron_scheduler::{Job, JobScheduler};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledQuery {
    pub name: String,
    pub cron: String,
}

#[derive(Debug, Clone, Copy)]
struct SchedulerRuntimeConfig {
    max_concurrent_jobs: usize,
    job_timeout_minutes: Option<u32>,
}

pub async fn start(yetii: &Yetii, detach: bool, pid_file: &str, log_file: &str) -> Result<String> {
    if detach {
        return start_detached(yetii, pid_file, log_file);
    }

    run_foreground(pid_file).await
}

pub fn status(pid_file: &str) -> Result<String> {
    let pid = read_pid(pid_file)?;
    if process_is_running(pid) {
        Ok(format!("Yetii daemon is running with pid {pid}"))
    } else {
        Ok(format!(
            "Yetii daemon is not running; stale pid file contains pid {pid}"
        ))
    }
}

pub fn stop(pid_file: &str) -> Result<String> {
    let pid = read_pid(pid_file)?;
    if !process_is_running(pid) {
        let _ = remove_file(pid_file);
        return Ok(format!(
            "Yetii daemon was not running; removed stale pid file for pid {pid}"
        ));
    }

    stop_process(pid)?;
    let _ = remove_file(pid_file);
    Ok(format!("Stop signal sent to Yetii daemon pid {pid}"))
}

async fn run_foreground(pid_file: &str) -> Result<String> {
    ensure_no_running_pid(pid_file)?;
    write_pid_file(pid_file, std::process::id())?;

    let config = config::get_config()?.clone();
    let runtime = scheduler_runtime_config(config.execution.scheduler.as_ref())?;
    let scheduled_queries = scheduled_queries(&config.queries)?;
    if scheduled_queries.is_empty() {
        tracing::warn!("no enabled scheduled queries found");
    }

    let scheduler = JobScheduler::new()
        .await
        .context("failed to create scheduler")?;
    let semaphore = Arc::new(Semaphore::new(runtime.max_concurrent_jobs));

    for scheduled_query in scheduled_queries {
        let query_name = scheduled_query.name.clone();
        let cron = scheduled_query.cron.clone();
        let semaphore = semaphore.clone();
        let timeout_minutes = runtime.job_timeout_minutes;
        scheduler
            .add(Job::new_async(cron.clone(), move |_uuid, _lock| {
                let query_name = query_name.clone();
                let semaphore = semaphore.clone();
                Box::pin(async move {
                    let Ok(_permit) = semaphore.acquire_owned().await else {
                        tracing::error!(query = %query_name, "scheduler concurrency limiter was closed");
                        return;
                    };
                    run_scheduled_query(query_name, timeout_minutes).await;
                })
            })?)
            .await
            .with_context(|| format!("failed to register scheduled query '{}'", scheduled_query.name))?;
        tracing::info!(
            query = %scheduled_query.name,
            cron = %scheduled_query.cron,
            "scheduled query registered"
        );
    }

    scheduler
        .start()
        .await
        .context("failed to start scheduler")?;
    tracing::info!(
        pid = std::process::id(),
        pid_file,
        max_concurrent_jobs = runtime.max_concurrent_jobs,
        "Yetii daemon started"
    );

    tokio::signal::ctrl_c()
        .await
        .context("failed to listen for Ctrl-C")?;
    tracing::info!("shutdown signal received");
    let mut scheduler = scheduler;
    scheduler
        .shutdown()
        .await
        .context("failed to shut down scheduler")?;
    remove_pid_file_if_current(pid_file);
    Ok("Yetii daemon stopped".to_string())
}

async fn run_scheduled_query(query_name: String, timeout_minutes: Option<u32>) {
    let started = Instant::now();
    tracing::info!(query = %query_name, "scheduled query started");
    let run_future = run::run(Some(&query_name), false);
    let result = if let Some(timeout_minutes) = timeout_minutes.filter(|value| *value > 0) {
        match tokio::time::timeout(
            std::time::Duration::from_secs(timeout_minutes as u64 * 60),
            run_future,
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                tracing::error!(
                    query = %query_name,
                    timeout_minutes,
                    "scheduled query timed out"
                );
                return;
            }
        }
    } else {
        run_future.await
    };

    match result {
        Ok(report) if report.failures.is_empty() => tracing::info!(
            query = %query_name,
            rows_read = report.rows_read,
            batches_sent = report.batches_sent,
            duration_ms = started.elapsed().as_millis(),
            "scheduled query completed"
        ),
        Ok(report) => tracing::error!(
            query = %query_name,
            rows_read = report.rows_read,
            batches_sent = report.batches_sent,
            failures = report.failures.len(),
            duration_ms = started.elapsed().as_millis(),
            "scheduled query completed with failures"
        ),
        Err(error) => tracing::error!(
            query = %query_name,
            error = %error,
            duration_ms = started.elapsed().as_millis(),
            "scheduled query failed"
        ),
    }
}

pub fn scheduled_queries(queries: &[QueryConfig]) -> Result<Vec<ScheduledQuery>> {
    queries
        .iter()
        .filter(|query| query.enabled)
        .filter_map(|query| {
            let schedule = query.schedule.as_ref()?;
            schedule.enabled.then_some((query, schedule))
        })
        .map(|(query, schedule)| {
            Ok(ScheduledQuery {
                name: query.name.clone(),
                cron: normalized_cron(&schedule.cron)?,
            })
        })
        .collect()
}

fn scheduler_runtime_config(scheduler: Option<&SchedulerConfig>) -> Result<SchedulerRuntimeConfig> {
    let Some(scheduler) = scheduler else {
        return Ok(SchedulerRuntimeConfig {
            max_concurrent_jobs: 1,
            job_timeout_minutes: None,
        });
    };

    if !scheduler.enabled {
        bail!("scheduler is disabled in execution.scheduler.enabled");
    }
    if scheduler.max_concurrent_jobs == 0 {
        bail!("execution.scheduler.max_concurrent_jobs must be greater than zero");
    }
    if scheduler.missed_job_policy != "skip" {
        bail!("only execution.scheduler.missed_job_policy=skip is supported currently");
    }

    Ok(SchedulerRuntimeConfig {
        max_concurrent_jobs: scheduler.max_concurrent_jobs as usize,
        job_timeout_minutes: Some(scheduler.job_timeout_minutes),
    })
}

fn start_detached(yetii: &Yetii, pid_file: &str, log_file: &str) -> Result<String> {
    ensure_no_running_pid(pid_file)?;
    ensure_parent_dir(pid_file)?;
    ensure_parent_dir(log_file)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
        .with_context(|| format!("failed to open daemon log file '{log_file}'"))?;
    let log_for_stderr = log
        .try_clone()
        .context("failed to clone daemon log file handle")?;
    let exe = std::env::current_exe().context("failed to determine current executable")?;
    let mut command = Command::new(exe);
    command
        .arg("--file")
        .arg(&yetii.file)
        .args(yetii.verbose.then_some("--verbose"))
        .arg("daemon")
        .arg("start")
        .arg("--pid-file")
        .arg(pid_file)
        .arg("--log-file")
        .arg(log_file)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_for_stderr));

    configure_detached_process(&mut command);
    let child = command.spawn().context("failed to spawn detached daemon")?;
    write_pid_file(pid_file, child.id())?;
    Ok(format!(
        "Yetii daemon started in detached mode with pid {}; logs: {}",
        child.id(),
        log_file
    ))
}

fn ensure_no_running_pid(pid_file: &str) -> Result<()> {
    match read_pid(pid_file) {
        Ok(pid) if process_is_running(pid) => {
            bail!("Yetii daemon already appears to be running with pid {pid}")
        }
        Ok(_) => {
            let _ = remove_file(pid_file);
            Ok(())
        }
        Err(_) => Ok(()),
    }
}

fn read_pid(pid_file: &str) -> Result<u32> {
    let content = read_to_string(pid_file)
        .with_context(|| format!("failed to read pid file '{pid_file}'"))?;
    content
        .trim()
        .parse::<u32>()
        .with_context(|| format!("pid file '{pid_file}' does not contain a valid pid"))
}

fn write_pid_file(pid_file: &str, pid: u32) -> Result<()> {
    ensure_parent_dir(pid_file)?;
    std::fs::write(pid_file, format!("{pid}\n"))
        .with_context(|| format!("failed to write pid file '{pid_file}'"))
}

fn remove_pid_file_if_current(pid_file: &str) {
    let current_pid = std::process::id();
    if read_pid(pid_file).is_ok_and(|pid| pid == current_pid) {
        let _ = remove_file(pid_file);
    }
}

fn ensure_parent_dir(path: &str) -> Result<()> {
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory '{}'", parent.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn configure_detached_process(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn configure_detached_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn configure_detached_process(_command: &mut Command) {}

#[cfg(unix)]
fn process_is_running(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
fn process_is_running(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}")])
        .output()
        .is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
        })
}

#[cfg(not(any(unix, windows)))]
fn process_is_running(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
fn stop_process(pid: u32) -> Result<()> {
    let status = Command::new("kill")
        .arg(pid.to_string())
        .status()
        .context("failed to run kill")?;
    if !status.success() {
        bail!("kill failed for pid {pid}");
    }
    Ok(())
}

#[cfg(windows)]
fn stop_process(pid: u32) -> Result<()> {
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T"])
        .status()
        .context("failed to run taskkill")?;
    if !status.success() {
        bail!("taskkill failed for pid {pid}");
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn stop_process(pid: u32) -> Result<()> {
    bail!("stopping daemon pid {pid} is not supported on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::endpoint_config::EndpointConfig;
    use crate::config::schedule_config::ScheduleConfig;
    use crate::config::sql_query::SqlQuery;
    use crate::config::transform_config::TransformConfig;

    #[test]
    fn only_enabled_scheduled_queries_are_selected() {
        let selected = scheduled_queries(&[
            query("scheduled", true, Some(schedule(true))),
            query("query_disabled", false, Some(schedule(true))),
            query("schedule_disabled", true, Some(schedule(false))),
            query("manual", true, None),
        ])
        .unwrap();

        assert_eq!(
            vec![ScheduledQuery {
                name: "scheduled".to_string(),
                cron: "0 */5 * * * *".to_string()
            }],
            selected
        );
    }

    #[test]
    fn scheduler_runtime_rejects_unsupported_missed_policy() {
        let config = SchedulerConfig {
            enabled: true,
            max_concurrent_jobs: 1,
            job_timeout_minutes: 30,
            missed_job_policy: "replay".to_string(),
        };

        assert!(scheduler_runtime_config(Some(&config)).is_err());
    }

    #[test]
    fn scheduler_runtime_rejects_zero_concurrency() {
        let config = SchedulerConfig {
            enabled: true,
            max_concurrent_jobs: 0,
            job_timeout_minutes: 30,
            missed_job_policy: "skip".to_string(),
        };

        assert!(scheduler_runtime_config(Some(&config)).is_err());
    }

    fn schedule(enabled: bool) -> ScheduleConfig {
        ScheduleConfig {
            cron: "*/5 * * * *".to_string(),
            timezone: "UTC".to_string(),
            enabled,
        }
    }

    fn query(name: &str, enabled: bool, schedule: Option<ScheduleConfig>) -> QueryConfig {
        QueryConfig {
            name: name.to_string(),
            description: String::new(),
            enabled,
            database: None,
            schedule,
            query: SqlQuery {
                sql: "SELECT 1".to_string(),
                parameters: None,
                validation: None,
            },
            transform: TransformConfig::default(),
            endpoint: EndpointConfig {
                url: "http://127.0.0.1/sync".to_string(),
                method: "POST".to_string(),
                auth: None,
                headers: None,
                request: Default::default(),
                response: None,
            },
        }
    }
}
