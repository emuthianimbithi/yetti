pub mod conn_string;
pub mod error;

use crate::config::database::DatabaseConfig;
use crate::config::sql_query::QueryParameter;
use chrono::Utc;
use conn_string::{build_connection_string, redacted_connection_description};
use error::DbError;
use odbc_api::buffers::{AnySlice, BufferDesc, ColumnarAnyBuffer};
use odbc_api::parameter::InputParameter;
use odbc_api::sys::{Date, Time, Timestamp};
use odbc_api::{
    Bit, ColumnDescription, Connection, ConnectionOptions, Cursor, Environment, IntoParameter,
    Nullability,
};
use once_cell::sync::OnceCell;
use serde_json::{Map, Number, Value};
use std::collections::{HashMap, HashSet};
use tokio::sync::{mpsc, oneshot};

static ODBC_ENV: OnceCell<Environment> = OnceCell::new();

pub type QueryParameters = HashMap<String, QueryParameter>;

#[derive(Debug, Clone)]
pub struct QueryRequest {
    pub sql: String,
    pub parameters: Option<QueryParameters>,
}

struct SessionCommand {
    query: QueryRequest,
    response: oneshot::Sender<Result<Vec<Map<String, Value>>, DbError>>,
}

pub struct QuerySession {
    sender: mpsc::UnboundedSender<SessionCommand>,
    _worker: tokio::task::JoinHandle<()>,
}

pub fn env() -> Result<&'static Environment, DbError> {
    ODBC_ENV.get_or_try_init(|| {
        // The process owns exactly one environment for the remainder of its lifetime.
        Environment::new().map_err(DbError::Environment)
    })
}

pub async fn open_session(db: &DatabaseConfig) -> Result<QuerySession, DbError> {
    let db = db.clone();
    let (sender, mut receiver) = mpsc::unbounded_channel::<SessionCommand>();
    let (ready_sender, ready_receiver) = oneshot::channel();
    let worker = tokio::task::spawn_blocking(move || {
        let connection_string = build_connection_string(&db);
        tracing::debug!(
            database = %db.name,
            connection = %redacted_connection_description(&db),
            "opening ODBC connection"
        );
        let connection = match env().and_then(|environment| {
            environment
                .connect_with_connection_string(&connection_string, ConnectionOptions::default())
                .map_err(DbError::Connect)
        }) {
            Ok(connection) => connection,
            Err(error) => {
                let _ = ready_sender.send(Err(error));
                return;
            }
        };
        if ready_sender.send(Ok(())).is_err() {
            return;
        }

        while let Some(command) = receiver.blocking_recv() {
            let rows = run_query_on_connection(
                &connection,
                &command.query.sql,
                command.query.parameters.as_ref(),
                db.pool.timeout_seconds.map(|timeout| timeout as usize),
            );
            let _ = command.response.send(rows);
        }
    });

    ready_receiver
        .await
        .map_err(|_| DbError::Worker("connection worker exited during startup".to_string()))??;
    Ok(QuerySession {
        sender,
        _worker: worker,
    })
}

impl QuerySession {
    pub async fn run(&self, query: QueryRequest) -> Result<Vec<Map<String, Value>>, DbError> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(SessionCommand { query, response })
            .map_err(|_| DbError::Worker("query worker is no longer running".to_string()))?;
        receiver
            .await
            .map_err(|_| DbError::Worker("query worker exited before responding".to_string()))?
    }
}

fn run_query_on_connection(
    connection: &Connection<'_>,
    sql: &str,
    parameters: Option<&QueryParameters>,
    timeout_seconds: Option<usize>,
) -> Result<Vec<Map<String, Value>>, DbError> {
    let prepared = prepare_sql_and_params(sql, parameters)?;
    let Some(cursor) = connection
        .execute(&prepared.sql, prepared.params.as_slice(), timeout_seconds)
        .map_err(DbError::Execute)?
    else {
        return Ok(Vec::new());
    };

    fetch_rows_typed(cursor)
}

struct PreparedQuery {
    sql: String,
    params: Vec<Box<dyn InputParameter>>,
}

fn prepare_sql_and_params(
    sql: &str,
    parameters: Option<&QueryParameters>,
) -> Result<PreparedQuery, DbError> {
    let Some(parameters) = parameters.filter(|parameters| !parameters.is_empty()) else {
        return Ok(PreparedQuery {
            sql: sql.to_string(),
            params: Vec::new(),
        });
    };

    let (sql, ordered_names) = rewrite_named_placeholders(sql, parameters);
    let ordered_names = if ordered_names.is_empty() {
        positional_parameter_names(&sql, parameters)?
    } else {
        ordered_names
    };

    let used = ordered_names.iter().collect::<HashSet<_>>();
    let unused = parameters
        .keys()
        .filter(|name| !used.contains(name))
        .cloned()
        .collect::<Vec<_>>();
    if !unused.is_empty() {
        return Err(DbError::Parameter(format!(
            "unused parameter(s): {}; use named placeholders like $name or :name",
            unused.join(", ")
        )));
    }

    let params = ordered_names
        .iter()
        .map(|name| {
            let parameter = parameters.get(name).ok_or_else(|| {
                DbError::Parameter(format!("parameter '{name}' is not configured"))
            })?;
            parameter_to_input(name, parameter)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PreparedQuery { sql, params })
}

fn rewrite_named_placeholders(sql: &str, parameters: &QueryParameters) -> (String, Vec<String>) {
    let mut output = String::with_capacity(sql.len());
    let mut names = Vec::new();
    let mut chars = sql.char_indices().peekable();
    let mut last = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some((index, ch)) = chars.next() {
        let next = chars.peek().map(|(_, ch)| *ch);

        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_block_comment {
            if ch == '*' && next == Some('/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_single_quote {
            if ch == '\'' {
                if next == Some('\'') {
                    chars.next();
                } else {
                    in_single_quote = false;
                }
            }
            continue;
        }
        if in_double_quote {
            if ch == '"' {
                in_double_quote = false;
            }
            continue;
        }

        match (ch, next) {
            ('-', Some('-')) => {
                chars.next();
                in_line_comment = true;
            }
            ('/', Some('*')) => {
                chars.next();
                in_block_comment = true;
            }
            ('\'', _) => in_single_quote = true,
            ('"', _) => in_double_quote = true,
            (':' | '$', _) => {
                if ch == ':' && next == Some(':') {
                    continue;
                }
                let name_start = index + ch.len_utf8();
                let name_end = identifier_end(sql, name_start);
                if name_end == name_start {
                    continue;
                }
                let name = &sql[name_start..name_end];
                if parameters.contains_key(name) {
                    output.push_str(&sql[last..index]);
                    output.push('?');
                    names.push(name.to_string());
                    last = name_end;
                    while let Some((peek_index, _)) = chars.peek() {
                        if *peek_index < name_end {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    output.push_str(&sql[last..]);
    (output, names)
}

fn identifier_end(sql: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in sql[start..].char_indices() {
        if offset == 0 && !(ch == '_' || ch.is_ascii_alphabetic()) {
            break;
        }
        if !(ch == '_' || ch.is_ascii_alphanumeric()) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn positional_parameter_names(
    sql: &str,
    parameters: &QueryParameters,
) -> Result<Vec<String>, DbError> {
    let placeholder_count = count_positional_placeholders(sql);
    if placeholder_count == 0 {
        return Err(DbError::Parameter(
            "parameters are configured, but the SQL has no named or positional placeholders"
                .to_string(),
        ));
    }

    if parameters.len() == 1 && placeholder_count == 1 {
        return Ok(vec![
            parameters.keys().next().expect("one parameter").clone(),
        ]);
    }

    let mut numeric_names = parameters.keys().collect::<Vec<_>>();
    numeric_names.sort();
    let expected = (1..=placeholder_count)
        .map(|index| index.to_string())
        .collect::<Vec<_>>();
    if numeric_names
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>()
        == expected
    {
        return Ok(expected);
    }

    Err(DbError::Parameter(format!(
        "SQL has {placeholder_count} positional placeholder(s), but parameter order is ambiguous; use $name/:name placeholders or numeric keys 1..{placeholder_count}"
    )))
}

fn count_positional_placeholders(sql: &str) -> usize {
    let mut count = 0;
    let mut chars = sql.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        let next = chars.peek().copied();
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_block_comment {
            if ch == '*' && next == Some('/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_single_quote {
            if ch == '\'' {
                if next == Some('\'') {
                    chars.next();
                } else {
                    in_single_quote = false;
                }
            }
            continue;
        }
        if in_double_quote {
            if ch == '"' {
                in_double_quote = false;
            }
            continue;
        }

        match (ch, next) {
            ('-', Some('-')) => {
                chars.next();
                in_line_comment = true;
            }
            ('/', Some('*')) => {
                chars.next();
                in_block_comment = true;
            }
            ('\'', _) => in_single_quote = true,
            ('"', _) => in_double_quote = true,
            ('?', _) => count += 1,
            _ => {}
        }
    }

    count
}

fn parameter_to_input(
    name: &str,
    parameter: &QueryParameter,
) -> Result<Box<dyn InputParameter>, DbError> {
    let value = resolve_parameter_value(name, parameter)?;
    let param_type = parameter.param_type.to_ascii_lowercase();

    match param_type.as_str() {
        "int" | "integer" => Ok(Box::new(value.parse::<i32>().map_err(|error| {
            DbError::Parameter(format!(
                "parameter '{name}' is not a valid integer: {error}"
            ))
        })?)),
        "bigint" | "long" => Ok(Box::new(value.parse::<i64>().map_err(|error| {
            DbError::Parameter(format!("parameter '{name}' is not a valid bigint: {error}"))
        })?)),
        "float" | "double" | "decimal" | "numeric" => {
            Ok(Box::new(value.parse::<f64>().map_err(|error| {
                DbError::Parameter(format!("parameter '{name}' is not a valid number: {error}"))
            })?))
        }
        "bool" | "boolean" => Ok(Box::new(Bit::from_bool(parse_bool(name, &value)?))),
        "string" | "text" | "date" | "time" | "timestamp" | "datetime" => {
            Ok(Box::new(value.into_parameter()))
        }
        other => Err(DbError::Parameter(format!(
            "unsupported parameter type '{other}' for parameter '{name}'"
        ))),
    }
}

fn resolve_parameter_value(name: &str, parameter: &QueryParameter) -> Result<String, DbError> {
    match parameter.source.as_deref() {
        Some("now") => Ok(Utc::now().to_rfc3339()),
        Some("env") => {
            let env_name = parameter.default.as_deref().ok_or_else(|| {
                DbError::Parameter(format!(
                    "parameter '{name}' uses source=env but default does not name an environment variable"
                ))
            })?;
            std::env::var(env_name).map_err(|_| {
                DbError::Parameter(format!(
                    "environment variable '{env_name}' for parameter '{name}' is not set"
                ))
            })
        }
        Some(source) if source.starts_with("env:") => {
            let env_name = source.trim_start_matches("env:");
            std::env::var(env_name).map_err(|_| {
                DbError::Parameter(format!(
                    "environment variable '{env_name}' for parameter '{name}' is not set"
                ))
            })
        }
        _ => parameter.default.clone().ok_or_else(|| {
            DbError::Parameter(format!(
                "parameter '{name}' has no supported source value and no default"
            ))
        }),
    }
}

fn parse_bool(name: &str, value: &str) -> Result<bool, DbError> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "1" => Ok(true),
        "false" | "f" | "no" | "n" | "0" => Ok(false),
        _ => Err(DbError::Parameter(format!(
            "parameter '{name}' is not a valid boolean: {value}"
        ))),
    }
}

fn fetch_rows_typed(mut cursor: impl Cursor) -> Result<Vec<Map<String, Value>>, DbError> {
    let column_count = cursor.num_result_cols().map_err(DbError::Fetch)? as u16;
    let mut columns = Vec::with_capacity(column_count as usize);
    let mut buffer_descs = Vec::with_capacity(column_count as usize);

    for index in 1..=column_count {
        let name = cursor.col_name(index).map_err(DbError::Fetch)?;
        let mut description = ColumnDescription::default();
        cursor
            .describe_col(index, &mut description)
            .map_err(DbError::Fetch)?;
        let nullable = matches!(
            description.nullability,
            Nullability::Unknown | Nullability::Nullable
        );
        let data_type = cursor.col_data_type(index).unwrap_or(description.data_type);
        let desc = BufferDesc::from_data_type(data_type, nullable).unwrap_or(BufferDesc::Text {
            max_str_len: 16 * 1024,
        });
        let desc = clamp_text_buffer(desc);
        columns.push(name);
        buffer_descs.push(desc);
    }

    let buffers = ColumnarAnyBuffer::try_from_descs(500, buffer_descs).map_err(DbError::Fetch)?;
    let mut row_set_cursor = cursor.bind_buffer(buffers).map_err(DbError::Fetch)?;
    let mut rows = Vec::new();

    while let Some(batch) = row_set_cursor.fetch().map_err(DbError::Fetch)? {
        for row_index in 0..batch.num_rows() {
            let mut row = Map::with_capacity(columns.len());
            for (column_index, column_name) in columns.iter().enumerate() {
                row.insert(
                    column_name.clone(),
                    any_value_to_json(batch.column(column_index), row_index),
                );
            }
            rows.push(row);
        }
    }

    Ok(rows)
}

fn clamp_text_buffer(desc: BufferDesc) -> BufferDesc {
    match desc {
        BufferDesc::Text { max_str_len } => BufferDesc::Text {
            max_str_len: max_str_len.clamp(1, 16 * 1024),
        },
        BufferDesc::WText { max_str_len } => BufferDesc::WText {
            max_str_len: max_str_len.clamp(1, 16 * 1024),
        },
        BufferDesc::Binary { length } => BufferDesc::Binary {
            length: length.clamp(1, 16 * 1024),
        },
        other => other,
    }
}

fn any_value_to_json(column: AnySlice<'_>, row_index: usize) -> Value {
    match column {
        AnySlice::Text(view) => view
            .get(row_index)
            .map(|bytes| Value::String(String::from_utf8_lossy(bytes).into_owned()))
            .unwrap_or(Value::Null),
        AnySlice::WText(view) => view
            .get(row_index)
            .map(|chars| Value::String(String::from_utf16_lossy(chars)))
            .unwrap_or(Value::Null),
        AnySlice::Binary(view) => view
            .get(row_index)
            .map(|bytes| Value::String(hex_encode(bytes)))
            .unwrap_or(Value::Null),
        AnySlice::Date(values) => Value::String(format_date(values[row_index])),
        AnySlice::Time(values) => Value::String(format_time(values[row_index])),
        AnySlice::Timestamp(values) => Value::String(format_timestamp(values[row_index])),
        AnySlice::F64(values) => number_or_string(values[row_index]),
        AnySlice::F32(values) => number_or_string(values[row_index] as f64),
        AnySlice::I8(values) => Value::Number(Number::from(values[row_index])),
        AnySlice::I16(values) => Value::Number(Number::from(values[row_index])),
        AnySlice::I32(values) => Value::Number(Number::from(values[row_index])),
        AnySlice::I64(values) => Value::Number(Number::from(values[row_index])),
        AnySlice::U8(values) => Value::Number(Number::from(values[row_index])),
        AnySlice::Bit(values) => Value::Bool(values[row_index].as_bool()),
        AnySlice::NullableDate(values) => values
            .into_iter()
            .nth(row_index)
            .flatten()
            .map(|value| Value::String(format_date(*value)))
            .unwrap_or(Value::Null),
        AnySlice::NullableTime(values) => values
            .into_iter()
            .nth(row_index)
            .flatten()
            .map(|value| Value::String(format_time(*value)))
            .unwrap_or(Value::Null),
        AnySlice::NullableTimestamp(values) => values
            .into_iter()
            .nth(row_index)
            .flatten()
            .map(|value| Value::String(format_timestamp(*value)))
            .unwrap_or(Value::Null),
        AnySlice::NullableF64(values) => values
            .into_iter()
            .nth(row_index)
            .flatten()
            .map(|value| number_or_string(*value))
            .unwrap_or(Value::Null),
        AnySlice::NullableF32(values) => values
            .into_iter()
            .nth(row_index)
            .flatten()
            .map(|value| number_or_string(*value as f64))
            .unwrap_or(Value::Null),
        AnySlice::NullableI8(values) => {
            nullable_number(values.into_iter().nth(row_index).flatten())
        }
        AnySlice::NullableI16(values) => {
            nullable_number(values.into_iter().nth(row_index).flatten())
        }
        AnySlice::NullableI32(values) => {
            nullable_number(values.into_iter().nth(row_index).flatten())
        }
        AnySlice::NullableI64(values) => {
            nullable_number(values.into_iter().nth(row_index).flatten())
        }
        AnySlice::NullableU8(values) => {
            nullable_number(values.into_iter().nth(row_index).flatten())
        }
        AnySlice::NullableBit(values) => values
            .into_iter()
            .nth(row_index)
            .flatten()
            .map(|value| Value::Bool(value.as_bool()))
            .unwrap_or(Value::Null),
    }
}

fn nullable_number<T>(value: Option<&T>) -> Value
where
    Number: From<T>,
    T: Copy,
{
    value
        .map(|value| Value::Number(Number::from(*value)))
        .unwrap_or(Value::Null)
}

fn number_or_string(value: f64) -> Value {
    Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or_else(|| Value::String(value.to_string()))
}

fn format_date(value: Date) -> String {
    format!("{:04}-{:02}-{:02}", value.year, value.month, value.day)
}

fn format_time(value: Time) -> String {
    format!("{:02}:{:02}:{:02}", value.hour, value.minute, value.second)
}

fn format_timestamp(value: Timestamp) -> String {
    if value.fraction == 0 {
        format!(
            "{}T{}",
            format_date(Date {
                year: value.year,
                month: value.month,
                day: value.day,
            }),
            format_time(Time {
                hour: value.hour,
                minute: value.minute,
                second: value.second,
            })
        )
    } else {
        format!(
            "{}T{}.{:09}",
            format_date(Date {
                year: value.year,
                month: value.month,
                day: value.day,
            }),
            format_time(Time {
                hour: value.hour,
                minute: value.minute,
                second: value.second,
            }),
            value.fraction
        )
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(param_type: &str, default: &str) -> QueryParameter {
        QueryParameter {
            param_type: param_type.to_string(),
            default: Some(default.to_string()),
            source: None,
        }
    }

    #[test]
    fn rewrites_named_placeholders_in_sql_order() {
        let mut params = QueryParameters::new();
        params.insert(
            "after".to_string(),
            param("timestamp", "2026-01-01T00:00:00Z"),
        );
        params.insert("tenant".to_string(), param("string", "acme"));

        let prepared = prepare_sql_and_params(
            "SELECT * FROM orders WHERE updated_at > $after AND tenant = :tenant",
            Some(&params),
        )
        .unwrap();

        assert_eq!(
            "SELECT * FROM orders WHERE updated_at > ? AND tenant = ?",
            prepared.sql
        );
        assert_eq!(2, prepared.params.len());
    }

    #[test]
    fn ignores_placeholder_like_text_inside_strings_and_comments() {
        let mut params = QueryParameters::new();
        params.insert("real".to_string(), param("string", "value"));
        let (sql, names) = rewrite_named_placeholders(
            "SELECT '$fake', col::text FROM t -- :fake\nWHERE col = :real",
            &params,
        );

        assert_eq!(
            "SELECT '$fake', col::text FROM t -- :fake\nWHERE col = ?",
            sql
        );
        assert_eq!(vec!["real"], names);
    }

    #[test]
    fn rejects_ambiguous_positional_parameters() {
        let mut params = QueryParameters::new();
        params.insert("left".to_string(), param("string", "a"));
        params.insert("right".to_string(), param("string", "b"));

        assert!(prepare_sql_and_params("SELECT ? = ?", Some(&params)).is_err());
    }

    #[test]
    fn numeric_keys_allow_positional_parameters() {
        let mut params = QueryParameters::new();
        params.insert("1".to_string(), param("string", "a"));
        params.insert("2".to_string(), param("string", "b"));

        let prepared = prepare_sql_and_params("SELECT ? = ?", Some(&params)).unwrap();

        assert_eq!("SELECT ? = ?", prepared.sql);
        assert_eq!(2, prepared.params.len());
    }
}
