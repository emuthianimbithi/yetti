# Notification services

Yetii can send best-effort notifications for runtime events. Notifications are intended for alerts, audit events, and operational integrations. A notification delivery failure is logged but does not fail the data sync.

## Service-based configuration

Use `monitoring.notifications.services` for new integrations:

```yaml
monitoring:
  enabled: true
  notifications:
    enabled: true
    services:
      - name: ops_api
        type: http
        enabled: true
        events:
          - query_failure
          - query_success

        endpoint:
          url: https://ops.example.com/yetii/events
          method: POST

        auth:
          type: bearer
          token: ${OPS_API_TOKEN}

        headers:
          X-Source: yetii
          X-Environment: production

        payload:
          format: json
          template:
            app: yetii
            event: "{{event}}"
            query: "{{query_name}}"
            status: "{{status}}"
            rows_read: "{{rows_read}}"
            pages_read: "{{pages_read}}"
            batches_sent: "{{batches_sent}}"
            failures: "{{failures}}"
            duration_ms: "{{duration_ms}}"
            error: "{{error}}"
            occurred_at: "{{occurred_at}}"

        response:
          success_codes: [200, 201, 202, 204]

        retry:
          attempts: 3
          delay_seconds: 5
          backoff: exponential
          timeout_seconds: 30
```

Currently `type: http` is supported.

## Events

The supported event names are:

- `query_success`
- `query_failure`
- `run_success`
- `run_failure`
- `daemon_started`
- `daemon_stopping`

The runtime emits query events during each query outcome, run events after the selected run finishes, and daemon lifecycle events when the foreground daemon becomes ready or starts graceful shutdown.

## Auth

Notification services reuse the same auth shapes as HTTP endpoints:

```yaml
auth:
  type: bearer
  token: ${TOKEN}
```

```yaml
auth:
  type: api_key
  header_name: X-API-Key
  token: ${TOKEN}
```

```yaml
auth:
  type: basic
  username: ${USER}
  password: ${PASSWORD}
```

OAuth2 client-credentials auth is also accepted with the same fields used by endpoint delivery.

## Template fields

Payload templates are JSON values. Placeholders are resolved from a controlled event context; Yetii does not execute scripts from YAML.

Available placeholders:

- `event`
- `success`
- `status`
- `query`
- `query_name`
- `rows_read`
- `pages_read`
- `batches_sent`
- `failures`
- `duration_ms`
- `error`
- `environment`
- `occurred_at`
- `started_at`
- `finished_at`

If a field is exactly one placeholder, Yetii preserves the JSON type:

```yaml
payload:
  format: json
  template:
    rows_read: "{{rows_read}}"
```

This sends a number:

```json
{
  "rows_read": 1500
}
```

If the placeholder is embedded in a longer string, Yetii sends a string:

```yaml
payload:
  format: json
  template:
    message: "query {{query_name}} failed: {{error}}"
```

Unknown placeholders are configuration/runtime errors for that notification service.

## Legacy webhook channels

The older form still works:

```yaml
monitoring:
  enabled: true
  notifications:
    on_failure: true
    on_success: false
    channels:
      - type: webhook
        url: ${FAILURE_WEBHOOK_URL}
```

This sends the default Yetii event JSON to the webhook. New integrations should use `services` because they support event filters, auth, headers, custom DTOs, response success codes, and retries.

Email channels remain validation-only. For production email alerts today, use an HTTP email provider API through `services`. Native SMTP delivery can be added later once SMTP host, port, auth, sender, TLS, and timeout settings are represented explicitly in config.
