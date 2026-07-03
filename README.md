<p align="center">
  <img src="logo.png" alt="YETII Logo" width="500" style="border-radius: 20px;" />
</p>

# Yetii

Yetii is a YAML-driven database-to-HTTP sync runner. It reads rows from ODBC databases, transforms them, sends them to HTTP endpoints, and can run once or continuously as a scheduler daemon.

It is designed for practical integration work:

- ODBC database access for PostgreSQL, MySQL/MariaDB, SQL Server, Oracle, and custom ODBC drivers
- YAML configuration for databases, queries, transforms, endpoints, scheduling, state, monitoring, and notifications
- Safe bound SQL parameters
- Batched HTTP delivery with retries and auth
- Incremental sync with state-file watermarks
- Foreground or detached daemon mode
- Health and Prometheus metrics endpoints
- Pluggable HTTP notification services
- Docker image, Docker Compose smoke stack, and GitHub Actions image publishing workflow

## Quick start with Docker

Build locally:

```bash
docker build -t yetii:local .
```

Validate a config:

```bash
docker run --rm \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  -e ERP_PASSWORD=secret \
  -e API_TOKEN=secret \
  yetii:local check-config
```

Verify the YAML-selected ODBC drivers exist in the image:

```bash
docker run --rm \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  yetii:local setup --check-only
```

Run one query:

```bash
docker run --rm \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  -v yetii-state:/var/lib/yetii \
  -e ERP_PASSWORD=secret \
  -e API_TOKEN=secret \
  yetii:local run --query orders_sync
```

Run daemon mode:

```bash
docker run -d \
  --name yetii \
  --restart unless-stopped \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  -v yetii-state:/var/lib/yetii \
  -e ERP_PASSWORD=secret \
  -e API_TOKEN=secret \
  -p 8080:8080 \
  -p 9090:9090 \
  yetii:local
```

Inside Docker, the default command is:

```bash
yetii --file /etc/yetii/yetii.yaml daemon start
```

Do not use `daemon start --detach` inside Docker. Docker is already the process manager.

## Pulling published images

The CI workflow publishes to GitHub Container Registry:

```bash
docker pull ghcr.io/<github-owner>/yetii:latest
```

Example:

```bash
docker run --rm \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  -v yetii-state:/var/lib/yetii \
  -e ERP_PASSWORD=secret \
  -e API_TOKEN=secret \
  ghcr.io/<github-owner>/yetii:latest run
```

The image name used by CI is:

```text
ghcr.io/${{ github.repository_owner }}/yetii
```

See [Docker deployment](docs/docker.md) for full Docker, Compose, state-volume, driver, and cloud database notes.

## End-to-end Docker Compose proof

The repo includes a local smoke stack:

```text
Postgres container → Yetii container over ODBC → mock HTTP receiver container
```

Run it:

```bash
cd examples/docker-compose
docker compose up --build --abort-on-container-exit --exit-code-from yetii
```

Inspect the receiver:

```bash
docker compose logs mock-receiver
```

Clean up:

```bash
docker compose down -v
```

Files:

- [examples/docker-compose/compose.yaml](examples/docker-compose/compose.yaml)
- [examples/docker-compose/yetii.yaml](examples/docker-compose/yetii.yaml)
- [examples/docker-compose/init.sql](examples/docker-compose/init.sql)
- [examples/docker-compose/mock-receiver/server.js](examples/docker-compose/mock-receiver/server.js)

## Native installation

Prerequisites:

- Rust toolchain
- unixODBC or the platform ODBC manager
- database-specific ODBC drivers

Build:

```bash
cargo build --release
```

Run in development:

```bash
cargo run -- --file yetii.yaml check-config
cargo run -- --file yetii.yaml run --query orders_sync
```

List installed ODBC drivers:

```bash
cargo run -- odbc
```

## Commands

### `init`

Create a starter configuration:

```bash
yetii init --path .
```

The config filename comes from `--file`; by default it writes `yetii.yaml`.

### `odbc`

List ODBC driver manager details:

```bash
yetii odbc
```

This command does not require a Yetii YAML file.

### `setup`

Read configured databases and drivers from YAML, then install or verify supported ODBC prerequisites.

```bash
# Preview system changes
yetii --file yetii.yaml setup --dry-run

# Verify drivers are already registered
yetii --file yetii.yaml setup --check-only

# Install supported prerequisites
yetii --file yetii.yaml setup
```

Supported automatic setup:

- macOS: Homebrew `unixodbc` and `psqlodbc`, including PostgreSQL driver registration
- Ubuntu/Debian: `unixodbc` and `odbc-postgresql`
- Fedora/RHEL, openSUSE, Arch, Alpine: corresponding unixODBC and PostgreSQL driver packages
- Windows: PostgreSQL or Microsoft SQL Server Driver 18 through WinGet

Custom drivers, Oracle Instant Client, and some MySQL packages produce explicit manual-install guidance instead of guessing a risky package.

For containers, prefer:

```bash
yetii setup --check-only
```

Production containers should already contain their required drivers.

### `check-config`

Validate YAML:

```bash
yetii --file yetii.yaml check-config
```

### `run`

Run one query:

```bash
yetii --file yetii.yaml run --query orders_sync
```

Run all enabled queries:

```bash
yetii --file yetii.yaml run
```

Run a disabled query intentionally:

```bash
yetii --file yetii.yaml run --query orders_sync --force
```

Run behavior:

- selects one query or all enabled queries
- opens ODBC connections inside blocking workers
- reuses one ODBC session per database during a run
- binds SQL parameters safely
- extracts typed JSON values where supported
- applies transforms
- chunks rows by `endpoint.request.batch_size`
- sends batches to HTTP endpoints
- retries transient HTTP failures
- updates incremental state only after successful delivery
- emits structured JSON logs and a summary

### `daemon`

Run scheduled queries in the foreground:

```bash
yetii --file yetii.yaml daemon start
```

Run detached on a host:

```bash
yetii --file yetii.yaml daemon start --detach \
  --pid-file .yetii/yetii.pid \
  --log-file .yetii/yetii.log
```

Check or stop:

```bash
yetii daemon status --pid-file .yetii/yetii.pid
yetii daemon stop --pid-file .yetii/yetii.pid
```

Daemon behavior:

- schedules queries with five-field or six-field cron expressions
- respects `execution.scheduler.max_concurrent_jobs`
- skips overlapping runs of the same query
- currently requires `missed_job_policy: skip`
- handles Ctrl+C and `SIGTERM` gracefully
- stops accepting new scheduled jobs during shutdown
- waits for active jobs to finish
- removes stale PID files from `daemon status`

## Configuration overview

Minimal shape:

```yaml
version: "1.0.0"
name: production-sync

databases:
  - name: erp
    type: postgres
    driver: PostgreSQL Unicode
    host: erp-db.example.com
    port: 5432
    database: erp
    connection_options:
      SSLmode: require
    auth:
      username: sync_user
      password: ${ERP_PASSWORD}

queries:
  - name: orders_sync
    description: Send changed orders to the API.
    enabled: true
    database: erp
    query:
      sql: |
        SELECT id, status, updated_at
        FROM orders
        ORDER BY updated_at, id
        LIMIT 1000
      parameters: null
      validation: null
    transform:
      enabled: true
      mappings:
        id: order_id
        status: order_status
      filters: []
      conversions: null
      group_by: null
    endpoint:
      url: https://api.example.com/orders
      method: POST
      auth:
        type: bearer
        token: ${API_TOKEN}
      headers:
        X-Source: yetii
      request:
        format: json
        batch_size: 100
        timeout_seconds: 30
        retry_attempts: 3
        retry_delay_seconds: 1
        retry_backoff: exponential
      response:
        success_codes: [200, 201, 202, 204]
        handle_duplicates: skip

execution:
  mode: sequential
  state_management:
    enabled: true
    state_file: /var/lib/yetii/yetii_state.json
    backup_states: 5
  scheduler:
    enabled: true
    max_concurrent_jobs: 2
    job_timeout_minutes: 30
    missed_job_policy: skip
```

Important rules:

- `${ENV_VAR}` references are resolved when Yetii loads the YAML.
- Do not commit secrets into YAML.
- With multiple databases, every query must set `database`.
- `connection_string` can be used as a power-user escape hatch.
- If `connection_string` is used, set `driver` too when you want `setup --check-only` to verify the driver.
- Use `connection_options` for driver-specific options such as `SSLmode`, `Encrypt`, and `TrustServerCertificate`.

## Database and ODBC notes

Default driver names:

| Database type | Default ODBC driver |
| --- | --- |
| `postgres` | `PostgreSQL Unicode` |
| `mysql` | `MySQL ODBC 8.0 Unicode Driver` |
| `mssql` | `ODBC Driver 18 for SQL Server` |
| `oracle` | `Oracle in instantclient` |

Override with:

```yaml
databases:
  - name: erp
    type: postgres
    driver: PostgreSQL Unicode
```

Docker image bundled drivers:

- PostgreSQL ANSI
- PostgreSQL Unicode
- MariaDB Unicode
- `MySQL ODBC 8.0 Unicode Driver` compatibility registration pointing at the bundled MariaDB/MySQL-compatible driver

SQL Server and Oracle commonly require vendor package repositories, license acceptance, or Instant Client files. Use a custom image for those drivers. See [Docker deployment](docs/docker.md).

Cloud-hosted databases work if the container or host has DNS, network routing, firewall/security-group access, trusted CA certificates, and driver support for the required TLS/auth mode.

## HTTP delivery

Endpoint auth supports:

- bearer token
- API key header
- basic auth
- OAuth2 client credentials

Example OAuth2 endpoint:

```yaml
endpoint:
  url: https://api.example.com/rows
  method: POST
  auth:
    type: oauth2
    client_id: ${API_CLIENT_ID}
    client_secret: ${API_CLIENT_SECRET}
    token_url: https://api.example.com/oauth/token
    scopes:
      - rows.write
  request:
    format: json
    batch_size: 100
    retry_attempts: 3
    retry_delay_seconds: 1
    retry_backoff: exponential
  response:
    success_codes: [200, 201, 202, 204]
    handle_duplicates: skip
```

## Incremental sync and state

Incremental sync is explicit. Yetii does not invent a `WHERE` clause, `LIMIT`, or cursor field.

The query author must:

1. choose the cursor column or columns;
2. write the SQL condition;
3. bind saved state through `source: state_file`;
4. return the watermark columns in the result;
5. configure `watermark`.

Single cursor example:

```yaml
query:
  sql: |
    SELECT id, status, updated_at
    FROM orders
    WHERE updated_at > $last_run_time
    ORDER BY updated_at, id
  parameters:
    last_run_time:
      type: timestamp
      source: state_file
      default: "1970-01-01T00:00:00Z"

watermark:
  strategy: max
  column: updated_at
  parameter: last_run_time
```

Composite cursor example for duplicate timestamps:

```yaml
query:
  sql: |
    SELECT id, status, updated_at
    FROM orders
    WHERE
      (updated_at > $last_updated_at)
      OR (updated_at = $last_updated_at AND id > $last_id)
    ORDER BY updated_at, id
    LIMIT 1000
  parameters:
    last_updated_at:
      type: timestamp
      source: state_file
      default: "1970-01-01T00:00:00Z"
    last_id:
      type: integer
      source: state_file
      default: "0"

watermark:
  strategy: max_tuple
  columns: [updated_at, id]
  parameters: [last_updated_at, last_id]
  page_size: 1000
```

State guarantees:

- state advances only after every transformed HTTP batch succeeds
- empty results do not advance the watermark
- missing or null watermark columns fail the run
- backups rotate as `yetii_state.json.1`, up to `backup_states`
- concurrent jobs in one Yetii process cannot move a watermark backwards
- multiple Yetii processes must not share the same state file

Delivery is at-least-once. If some batches succeed and a later batch fails, the next run can resend earlier rows. Receiving APIs should support idempotency or upserts.

For full details and database-specific SQL examples, see [Incremental synchronization](docs/incremental-sync.md).

## Monitoring

Daemon mode can expose health and metrics:

```yaml
monitoring:
  enabled: true
  health_check:
    enabled: true
    endpoint: /health
    port: 8080
  metrics:
    enabled: true
    endpoint: http://127.0.0.1:9090/metrics
    interval_seconds: 30
```

Health:

```bash
curl -fsS http://localhost:8080/health
```

Metrics:

```bash
curl -fsS http://localhost:9090/metrics
```

`/health` returns `200` only when the daemon is ready and not shutting down. `/metrics` is Prometheus text format. `interval_seconds` is retained for compatibility; Prometheus still controls scrape frequency.

## Notifications

Use service-based notifications for alerts, audit events, Slack/Teams/Discord webhooks, PagerDuty-style APIs, email-provider APIs, or internal ops APIs:

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
          - run_failure
          - daemon_stopping
        endpoint:
          url: https://ops.example.com/yetii/events
          method: POST
        auth:
          type: bearer
          token: ${OPS_API_TOKEN}
        headers:
          X-Source: yetii
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

Notifications are best-effort. A notification failure is logged but does not fail the data sync.

The legacy `notifications.channels` webhook shape still works, but new integrations should use `notifications.services`.

See [Notification services](docs/notifications.md).

## GitHub Actions and image publishing

The Docker workflow is [`.github/workflows/docker.yml`](.github/workflows/docker.yml).

It does the following:

- builds the Docker image on pull requests
- smoke-tests `--help`, `odbc`, `odbcinst -q -d`, `check-config`, and `setup --check-only`
- publishes to GHCR on `main`
- publishes version and `latest` tags on `v*` tags

Repository setting required for publishing:

```text
Settings → Actions → General → Workflow permissions → Read and write permissions
```

## Project layout

```text
yetii/
├── .github/workflows/docker.yml
├── Dockerfile
├── docker/entrypoint.sh
├── docs/
│   ├── docker.md
│   ├── incremental-sync.md
│   └── notifications.md
├── examples/docker-compose/
├── src/
│   ├── cli/
│   ├── commands/
│   ├── config/
│   ├── database/
│   ├── http/
│   ├── monitoring/
│   ├── notifications/
│   ├── state/
│   └── transform/
├── Cargo.toml
└── Cargo.lock
```

## Development checks

```bash
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
git diff --check
```

Docker checks:

```bash
docker build -t yetii:local .
docker run --rm yetii:local --help
docker run --rm yetii:local odbcinst -q -d
```

Compose proof:

```bash
cd examples/docker-compose
docker compose up --build --abort-on-container-exit --exit-code-from yetii
docker compose down -v
```

## Current status

Implemented:

- CLI commands: `init`, `odbc`, `setup`, `check-config`, `run`, `daemon`
- async runtime with blocking ODBC execution isolated in worker threads
- ODBC connection-string builder and redaction
- typed result extraction
- safe bound parameters
- batch HTTP delivery
- endpoint auth including OAuth2 client credentials
- retries and backoff
- transforms: filters, conversions, mappings
- scheduler daemon, detached mode, graceful shutdown, overlap prevention
- state-file incremental sync, backups, scalar and tuple watermarks
- health endpoint and Prometheus metrics
- pluggable HTTP notifications
- Docker image and entrypoint
- Docker Compose smoke stack
- GitHub Actions Docker build and GHCR publish workflow

Not yet implemented:

- full connection pool and multi-worker execution model
- advanced grouping/aggregation transforms
- generated configuration schema
- daemon hot reload
- SMTP-native email delivery

## Troubleshooting

Start with:

```bash
yetii --file yetii.yaml check-config
yetii odbc
yetii --file yetii.yaml setup --check-only
```

Common issues:

- missing ODBC driver: set `databases.driver` to the exact name from `yetii odbc`
- cloud DB connection failure: verify DNS, firewall/security group, TLS, and driver options from inside the host/container
- incremental sync repeats rows: receiving endpoint should be idempotent; Yetii provides at-least-once delivery
- Docker state resets: mount `/var/lib/yetii` or configure state into a persistent volume
- Docker Desktop cannot reach host DB with `localhost`: use `host.docker.internal` or a real hostname

## License

MIT © 2025 Emmanuel Muthiani
