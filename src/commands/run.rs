use crate::config;
use crate::config::query_config::QueryConfig;
use crate::database::{self, QueryRequest};
use crate::http::HttpSender;
use crate::monitoring;
use crate::notifications::{self, NotificationEvent};
use crate::state::{self, StateStore, WatermarkUpdate, YetiiState};
use crate::transform;
use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::time::Instant;

struct DeliveryOutcome {
    rows_read: usize,
    batches_sent: usize,
    watermark: Option<WatermarkUpdate>,
}

#[derive(Debug, Default)]
pub struct RunReport {
    pub rows_read: usize,
    pub pages_read: usize,
    pub batches_sent: usize,
    pub failures: Vec<RunFailure>,
}

#[derive(Debug)]
pub struct RunFailure {
    pub query: String,
    pub error: String,
}

impl fmt::Display for RunReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "rows_read={} pages_read={} batches_sent={} failures={}",
            self.rows_read,
            self.pages_read,
            self.batches_sent,
            self.failures.len()
        )
    }
}

pub async fn run(query_name: Option<&str>, force: bool) -> Result<RunReport> {
    let run_started = Instant::now();
    let config = config::get_config()?.clone();
    let selected_queries = select_queries(&config.queries, query_name, force)?;
    let state_store = config
        .execution
        .state_management
        .as_ref()
        .filter(|state_management| state_management.enabled)
        .map(StateStore::from_config);
    let mut state = match &state_store {
        Some(store) => {
            let state = store.load_or_default().with_context(|| {
                format!("failed to load state file '{}'", store.path().display())
            })?;
            tracing::debug!(path = %store.path().display(), "loaded run state");
            Some(state)
        }
        None => None,
    };
    let mut report = RunReport::default();
    let mut sessions = HashMap::new();

    for query in selected_queries {
        let started = Instant::now();
        let initial_rows = report.rows_read;
        let initial_pages = report.pages_read;
        let initial_batches = report.batches_sent;
        monitoring::query_started(&query.name);
        let database_config = resolve_database(&config.databases, query)?;
        if !sessions.contains_key(&database_config.name) {
            match database::open_session(database_config).await {
                Ok(session) => {
                    sessions.insert(database_config.name.clone(), session);
                }
                Err(error) => {
                    report.failures.push(RunFailure {
                        query: query.name.clone(),
                        error: format!("database connection failed: {error}"),
                    });
                    record_query_outcome(
                        config.monitoring.as_ref(),
                        query,
                        false,
                        &error.to_string(),
                        0,
                        0,
                        0,
                        started,
                    )
                    .await;
                    continue;
                }
            }
        }

        let session = sessions
            .get(&database_config.name)
            .expect("session was just initialized");
        let result = execute_query_pages(
            query,
            session,
            state_store.as_ref(),
            &mut state,
            &mut report,
        )
        .await;
        let rows = report.rows_read - initial_rows;
        let pages = report.pages_read - initial_pages;
        let batches = report.batches_sent - initial_batches;
        match result {
            Ok(()) => {
                record_query_outcome(
                    config.monitoring.as_ref(),
                    query,
                    true,
                    "",
                    rows,
                    pages,
                    batches,
                    started,
                )
                .await;
            }
            Err(error) => {
                tracing::error!(query = %query.name, error = %error, "query run failed");
                report.failures.push(RunFailure {
                    query: query.name.clone(),
                    error: format!("{error:#}"),
                });
                record_query_outcome(
                    config.monitoring.as_ref(),
                    query,
                    false,
                    &format!("{error:#}"),
                    rows,
                    pages,
                    batches,
                    started,
                )
                .await;
            }
        }
    }

    tracing::info!(
        rows_read = report.rows_read,
        batches_sent = report.batches_sent,
        failures = report.failures.len(),
        "run completed"
    );
    let run_event = NotificationEvent::run_outcome(
        report.failures.is_empty(),
        report.rows_read,
        report.pages_read,
        report.batches_sent,
        report.failures.len(),
        run_started.elapsed(),
    );
    if let Err(notification_error) =
        notifications::notify(config.monitoring.as_ref(), &run_event).await
    {
        tracing::warn!(
            error = %notification_error,
            "run notification delivery failed"
        );
    }
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
async fn record_query_outcome(
    monitoring_config: Option<&config::monitor_config::MonitoringConfig>,
    query: &QueryConfig,
    success: bool,
    error: &str,
    rows: usize,
    pages: usize,
    batches: usize,
    started: Instant,
) {
    let duration = started.elapsed();
    if success {
        monitoring::query_succeeded(&query.name, rows, pages, batches, duration);
    } else {
        monitoring::query_failed(&query.name, error, rows, pages, batches, duration);
    }
    let event = NotificationEvent::query_outcome(
        query.name.clone(),
        success,
        (!success).then(|| error.to_string()),
        rows,
        pages,
        batches,
        duration,
    );
    if let Err(notification_error) = notifications::notify(monitoring_config, &event).await {
        tracing::warn!(
            query = %query.name,
            error = %notification_error,
            "notification delivery failed"
        );
    }
}

async fn execute_query_pages(
    query: &QueryConfig,
    session: &database::QuerySession,
    state_store: Option<&StateStore>,
    state: &mut Option<YetiiState>,
    report: &mut RunReport,
) -> Result<()> {
    let started_at = Utc::now();
    let page_size = query
        .watermark
        .as_ref()
        .and_then(|watermark| watermark.page_size);
    let mut page = 0usize;
    let mut query_rows = 0usize;
    let mut query_batches = 0usize;

    loop {
        page += 1;
        let parameters = resolve_parameters(query, state.as_ref())?;
        let current_watermark = parameters
            .as_ref()
            .map(|parameters| state::current_watermark(query, parameters))
            .transpose()?
            .flatten();
        let rows = session
            .run(QueryRequest {
                sql: query.query.sql.clone(),
                parameters,
            })
            .await
            .with_context(|| format!("database query '{}' failed on page {page}", query.name))?;

        if let Some(page_size) = page_size
            && rows.len() > page_size
        {
            bail!(
                "query '{}' returned {} rows, exceeding watermark.page_size={page_size}; make the SQL limit match page_size",
                query.name,
                rows.len()
            );
        }
        if page > 1 && rows.is_empty() {
            break;
        }

        let delivery = deliver_query_rows(query, rows, current_watermark.as_ref()).await?;
        query_rows += delivery.rows_read;
        query_batches += delivery.batches_sent;
        report.rows_read += delivery.rows_read;
        report.pages_read += 1;
        report.batches_sent += delivery.batches_sent;

        if let Some(store) = state_store {
            *state = Some(
                store
                    .record_success(
                        &query.name,
                        started_at,
                        query_rows,
                        query_batches,
                        delivery.watermark.clone(),
                    )
                    .await
                    .with_context(|| {
                        format!("failed to save state file '{}'", store.path().display())
                    })?,
            );
        }

        let Some(page_size) = page_size else {
            break;
        };
        if delivery.rows_read < page_size {
            break;
        }
        if delivery.watermark.is_none() {
            bail!(
                "query '{}' returned a full page without an advancing watermark",
                query.name
            );
        }
        tracing::debug!(query = %query.name, page, "continuing paginated query");
    }

    Ok(())
}

fn resolve_database<'a>(
    databases: &'a config::database::DatabaseConfigs,
    query: &QueryConfig,
) -> Result<&'a config::database::DatabaseConfig> {
    databases
        .resolve_for_query(query.database.as_deref())
        .ok_or_else(|| {
            if databases.len() > 1 && query.database.is_none() {
                anyhow!(
                    "query '{}' must set database when multiple databases are configured",
                    query.name
                )
            } else {
                anyhow!(
                    "query '{}' references unknown database '{}'",
                    query.name,
                    query.database.as_deref().unwrap_or("<missing>")
                )
            }
        })
}

fn resolve_parameters(
    query: &QueryConfig,
    state: Option<&YetiiState>,
) -> Result<Option<database::QueryParameters>> {
    let mut parameters = query.query.parameters.clone();
    if state.is_none()
        && parameters.as_ref().is_some_and(|parameters| {
            parameters
                .values()
                .any(crate::config::watermark_config::is_state_parameter)
        })
    {
        bail!(
            "query '{}' uses a state_file parameter but state management is not enabled",
            query.name
        );
    }
    if let Some(state) = state {
        state::resolve_query_parameters(query, &mut parameters, state)
            .with_context(|| format!("failed to resolve state parameters for '{}'", query.name))?;
    }
    Ok(parameters)
}

async fn deliver_query_rows(
    query: &QueryConfig,
    rows: Vec<serde_json::Map<String, serde_json::Value>>,
    current_watermark: Option<&WatermarkUpdate>,
) -> Result<DeliveryOutcome> {
    tracing::info!(query = %query.name, "delivering query rows");
    let rows_read = rows.len();
    let watermark = state::extract_watermark(query, &rows)
        .with_context(|| format!("watermark extraction for query '{}' failed", query.name))?;
    if let (Some(next), Some(current)) = (&watermark, current_watermark)
        && state::compare_watermarks(next, current)? != Ordering::Greater
    {
        bail!(
            "query '{}' did not advance its watermark; verify the WHERE clause and cursor ordering",
            query.name
        );
    }
    let rows = transform::apply(rows, &query.transform)
        .with_context(|| format!("transform for query '{}' failed", query.name))?;
    let rows = rows.into_iter().map(Value::Object).collect::<Vec<_>>();
    let batch_size = query.endpoint.request.batch_size.unwrap_or(100) as usize;
    let sender = HttpSender::new(&query.endpoint.request).with_context(|| {
        format!(
            "HTTP client for query '{}' could not be created",
            query.name
        )
    })?;
    let mut batches_sent = 0;

    for (batch_index, batch) in rows.chunks(batch_size).enumerate() {
        let outcome = sender.send(&query.endpoint, batch).await.with_context(|| {
            format!(
                "delivery of query '{}' batch {} failed",
                query.name,
                batch_index + 1
            )
        })?;
        batches_sent += 1;
        tracing::debug!(
            query = %query.name,
            batch = batch_index + 1,
            rows = batch.len(),
            status = outcome.status.as_u16(),
            "batch delivered"
        );
    }

    tracing::info!(
        query = %query.name,
        rows_read,
        batches_sent,
        "query completed"
    );
    Ok(DeliveryOutcome {
        rows_read,
        batches_sent,
        watermark,
    })
}

fn select_queries<'a>(
    queries: &'a [QueryConfig],
    query_name: Option<&str>,
    force: bool,
) -> Result<Vec<&'a QueryConfig>> {
    if let Some(query_name) = query_name {
        let query = queries
            .iter()
            .find(|query| query.name == query_name)
            .ok_or_else(|| anyhow!("query '{query_name}' was not found"))?;
        if !query.enabled && !force {
            bail!("query '{query_name}' is disabled; pass --force to run it");
        }
        return Ok(vec![query]);
    }

    Ok(queries
        .iter()
        .filter(|query| query.enabled || force)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::connection_config::ConnectionConfig;
    use crate::config::database::{AuthConfig, DatabaseConfig, DatabaseConfigs, DatabaseType};
    use crate::config::endpoint_config::EndpointConfig;
    use crate::config::sql_query::{QueryParameter, SqlQuery};
    use crate::config::transform_config::TransformConfig;

    fn query(name: &str, enabled: bool) -> QueryConfig {
        QueryConfig {
            name: name.to_string(),
            description: String::new(),
            enabled,
            database: None,
            schedule: None,
            query: SqlQuery {
                sql: "SELECT 1".to_string(),
                parameters: None,
                validation: None,
            },
            watermark: None,
            transform: TransformConfig::default(),
            endpoint: EndpointConfig {
                url: "https://example.test".to_string(),
                method: "POST".to_string(),
                auth: None,
                headers: None,
                request: Default::default(),
                response: None,
            },
        }
    }

    #[test]
    fn all_queries_skip_disabled_unless_forced() {
        let queries = vec![query("enabled", true), query("disabled", false)];

        assert_eq!(1, select_queries(&queries, None, false).unwrap().len());
        assert_eq!(2, select_queries(&queries, None, true).unwrap().len());
    }

    #[test]
    fn named_disabled_query_requires_force() {
        let queries = vec![query("disabled", false)];

        assert!(select_queries(&queries, Some("disabled"), false).is_err());
        assert_eq!(
            1,
            select_queries(&queries, Some("disabled"), true)
                .unwrap()
                .len()
        );
    }

    #[test]
    fn groups_queries_by_resolved_database() {
        let databases = DatabaseConfigs::from(vec![database("erp"), database("billing")]);
        let mut erp_query = query("customers", true);
        erp_query.database = Some("erp".to_string());
        let mut billing_query = query("invoices", true);
        billing_query.database = Some("billing".to_string());

        assert_eq!(
            "erp",
            resolve_database(&databases, &erp_query).unwrap().name
        );
        assert_eq!(
            "billing",
            resolve_database(&databases, &billing_query).unwrap().name
        );
    }

    #[test]
    fn single_database_can_be_implicit() {
        let databases = DatabaseConfigs::from(database("main"));
        let query = query("sync", true);

        assert_eq!("main", resolve_database(&databases, &query).unwrap().name);
    }

    #[test]
    fn multiple_databases_require_explicit_query_database() {
        let databases = DatabaseConfigs::from(vec![database("erp"), database("billing")]);
        let query = query("sync", true);

        assert!(resolve_database(&databases, &query).is_err());
    }

    #[test]
    fn group_queries_resolves_state_parameters_before_database_execution() {
        let mut query = query("orders_sync", true);
        let mut parameters = HashMap::new();
        parameters.insert(
            "last_run_time".to_string(),
            QueryParameter {
                param_type: "timestamp".to_string(),
                default: Some("1970-01-01T00:00:00Z".to_string()),
                source: Some("state_file".to_string()),
            },
        );
        query.query.sql = "SELECT * FROM orders WHERE updated_at > $last_run_time".to_string();
        query.query.parameters = Some(parameters);

        let mut state = YetiiState::default();
        state
            .queries
            .entry("orders_sync".to_string())
            .or_default()
            .watermarks
            .insert(
                "last_run_time".to_string(),
                "2026-07-01T10:00:00Z".to_string(),
            );

        let parameters = resolve_parameters(&query, Some(&state)).unwrap();
        let parameter = parameters.as_ref().unwrap().get("last_run_time").unwrap();

        assert_eq!(Some("2026-07-01T10:00:00Z"), parameter.default.as_deref());
        assert_eq!(None, parameter.source.as_deref());
    }

    fn database(name: &str) -> DatabaseConfig {
        DatabaseConfig {
            name: name.to_string(),
            db_type: DatabaseType::Postgres,
            driver: None,
            connection_string: None,
            connection_options: Default::default(),
            host: "localhost".to_string(),
            port: 5432,
            database: "postgres".to_string(),
            schema: None,
            auth: AuthConfig {
                username: None,
                password: None,
            },
            pool: ConnectionConfig::default(),
        }
    }
}
