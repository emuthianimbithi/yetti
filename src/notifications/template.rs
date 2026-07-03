use anyhow::{Result, bail};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub fn render_template(template: &Value, fields: &HashMap<&'static str, Value>) -> Result<Value> {
    match template {
        Value::String(value) => render_string(value, fields),
        Value::Array(values) => values
            .iter()
            .map(|value| render_template(value, fields))
            .collect(),
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| Ok((key.clone(), render_template(value, fields)?)))
            .collect::<Result<Map<String, Value>>>()
            .map(Value::Object),
        other => Ok(other.clone()),
    }
}

fn render_string(value: &str, fields: &HashMap<&'static str, Value>) -> Result<Value> {
    if let Some(name) = exact_placeholder(value) {
        return fields
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown notification placeholder '{{{{{name}}}}}'"));
    }

    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            bail!("unterminated notification placeholder in '{value}'");
        };
        let name = after_start[..end].trim();
        let Some(field) = fields.get(name) else {
            bail!("unknown notification placeholder '{{{{{name}}}}}'");
        };
        output.push_str(&value_as_string(field));
        rest = &after_start[end + 2..];
    }
    output.push_str(rest);
    Ok(Value::String(output))
}

fn exact_placeholder(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    let inner = trimmed.strip_prefix("{{")?.strip_suffix("}}")?.trim();
    (!inner.is_empty()).then_some(inner)
}

fn value_as_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn exact_placeholder_preserves_json_type() {
        let mut fields = HashMap::new();
        fields.insert("rows_read", Value::from(42));

        let rendered = render_template(&Value::String("{{rows_read}}".to_string()), &fields)
            .expect("template should render");

        assert_eq!(Value::from(42), rendered);
    }

    #[test]
    fn embedded_placeholder_renders_as_string() {
        let mut fields = HashMap::new();
        fields.insert("query_name", Value::from("orders"));
        fields.insert("occurred_at", Value::from(Utc::now().to_rfc3339()));

        let rendered = render_template(
            &Value::String("query {{query_name}} failed at {{occurred_at}}".to_string()),
            &fields,
        )
        .expect("template should render");

        assert!(
            rendered
                .as_str()
                .unwrap()
                .contains("query orders failed at ")
        );
    }

    #[test]
    fn unknown_placeholder_fails() {
        let fields = HashMap::new();

        assert!(render_template(&Value::String("{{missing}}".to_string()), &fields).is_err());
    }
}
