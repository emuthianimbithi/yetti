use crate::config;
use crate::config::query_config::QueryConfig;
use crate::database::{self, QueryRequest};
use crate::http::HttpSender;
use crate::state::{self, QueryRunState, StateStore, YetiiState};
use crate::transform;
use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fmt;

type QueryGroups = BTreeMap<String, Vec<QueryRequest>>;
type QueryRunStates = HashMap<String, QueryRunState>;

#[derive(Debug, Default)]
pub struct RunReport {
    pub rows_read: usize,
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
            "rows_read={} batches_sent={} failures={}",
            self.rows_read,
            self.batches_sent,
            self.failures.len()
        )
    }
}

pub async fn run(query_name: Option<&str>, force: bool) -> Result<RunReport> {
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
    let query_lookup = selected_queries
        .iter()
        .map(|query| (query.name.clone(), *query))
        .collect::<HashMap<_, _>>();
    let (groups, state_runs) =
        group_queries_by_database(&config.databases, &selected_queries, state.as_ref())?;

    for (database_name, db_requests) in groups {
        let database_config = config
            .databases
            .get(&database_name)
            .expect("query grouping only uses known databases");
        let db_outcomes = database::run_queries(database_config, db_requests)
            .await
            .with_context(|| format!("database query execution failed for '{database_name}'"))?;

        for outcome in db_outcomes {
            let query = query_lookup
                .get(&outcome.name)
                .expect("database outcome should match selected query");

            match outcome.rows {
                Ok(rows) => match deliver_query_rows(query, rows).await {
                    Ok((rows_read, batches_sent)) => {
                        report.rows_read += rows_read;
                        report.batches_sent += batches_sent;
                        record_successful_query_state(
                            &state_store,
                            state.as_mut(),
                            &state_runs,
                            &query.name,
                            rows_read,
                            batches_sent,
                        )?;
                    }
                    Err(error) => {
                        tracing::error!(query = %query.name, error = %error, "query delivery failed");
                        report.failures.push(RunFailure {
                            query: query.name.clone(),
                            error: format!("{error:#}"),
                        });
                    }
                },
                Err(error) => {
                    tracing::error!(query = %query.name, error = %error, "query run failed");
                    report.failures.push(RunFailure {
                        query: query.name.clone(),
                        error: format!("{error:#}"),
                    });
                }
            }
        }
    }

    tracing::info!(
        rows_read = report.rows_read,
        batches_sent = report.batches_sent,
        failures = report.failures.len(),
        "run completed"
    );
    Ok(report)
}

fn group_queries_by_database(
    databases: &config::database::DatabaseConfigs,
    queries: &[&QueryConfig],
    state: Option<&YetiiState>,
) -> Result<(QueryGroups, QueryRunStates)> {
    let mut groups = BTreeMap::new();
    let mut state_runs = HashMap::new();

    for query in queries {
        let database = databases
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
            })?;
        let mut parameters = query.query.parameters.clone();
        let state_parameters = match state {
            Some(state) => state::resolve_query_parameters(query, &mut parameters, state)
                .with_context(|| {
                    format!("failed to resolve state parameters for '{}'", query.name)
                })?,
            None => Vec::new(),
        };

        groups
            .entry(database.name.clone())
            .or_insert_with(Vec::new)
            .push(QueryRequest {
                name: query.name.clone(),
                sql: query.query.sql.clone(),
                parameters,
            });
        state_runs.insert(
            query.name.clone(),
            QueryRunState {
                started_at: Utc::now(),
                state_parameters,
            },
        );
    }

    Ok((groups, state_runs))
}

fn record_successful_query_state(
    state_store: &Option<StateStore>,
    state: Option<&mut YetiiState>,
    state_runs: &QueryRunStates,
    query_name: &str,
    rows_read: usize,
    batches_sent: usize,
) -> Result<()> {
    let (Some(store), Some(state), Some(run_state)) =
        (state_store.as_ref(), state, state_runs.get(query_name))
    else {
        return Ok(());
    };

    state.record_success(
        query_name,
        run_state.started_at,
        Utc::now(),
        rows_read,
        batches_sent,
        &run_state.state_parameters,
    );
    store
        .save(state)
        .with_context(|| format!("failed to save state file '{}'", store.path().display()))?;
    tracing::debug!(query = %query_name, path = %store.path().display(), "saved query state");
    Ok(())
}

async fn deliver_query_rows(
    query: &QueryConfig,
    rows: Vec<serde_json::Map<String, serde_json::Value>>,
) -> Result<(usize, usize)> {
    tracing::info!(query = %query.name, "delivering query rows");
    let rows_read = rows.len();
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
    Ok((rows_read, batches_sent))
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

        let (groups, _) =
            group_queries_by_database(&databases, &[&erp_query, &billing_query], None).unwrap();

        assert_eq!(2, groups.len());
        assert_eq!("customers", groups["erp"][0].name);
        assert_eq!("invoices", groups["billing"][0].name);
    }

    #[test]
    fn single_database_can_be_implicit() {
        let databases = DatabaseConfigs::from(database("main"));
        let query = query("sync", true);

        let (groups, _) = group_queries_by_database(&databases, &[&query], None).unwrap();

        assert_eq!("sync", groups["main"][0].name);
    }

    #[test]
    fn multiple_databases_require_explicit_query_database() {
        let databases = DatabaseConfigs::from(vec![database("erp"), database("billing")]);
        let query = query("sync", true);

        assert!(group_queries_by_database(&databases, &[&query], None).is_err());
    }

    #[test]
    fn group_queries_resolves_state_parameters_before_database_execution() {
        let databases = DatabaseConfigs::from(database("main"));
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

        let (groups, state_runs) =
            group_queries_by_database(&databases, &[&query], Some(&state)).unwrap();
        let parameter = groups["main"][0]
            .parameters
            .as_ref()
            .unwrap()
            .get("last_run_time")
            .unwrap();

        assert_eq!(Some("2026-07-01T10:00:00Z"), parameter.default.as_deref());
        assert_eq!(None, parameter.source.as_deref());
        assert_eq!(
            "last_run_time",
            state_runs["orders_sync"].state_parameters[0].watermark_name
        );
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
