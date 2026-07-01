<p align="center">
  <img src="logo.png" alt="YETII Logo" width="500" style="border-radius: 20px;" />
</p>

# 🧊 Yetii (YAML Enterprise Transformation & Integration Interface) CLI

**Yetii** is a Rust CLI for reading database rows through ODBC and delivering them to HTTP endpoints using YAML configuration.

> ⚠️ Work-in-progress — expect evolving features and improvements.

---

## 🚀 Features

### ✅ `init` — Initialize Configuration

Create a starter YAML config file with default values for Yetii.

**Usage:**

```bash
yetii init --path .
```

**Options:**
* `--path, -p`: Directory where the config file is created (default: current directory)

This generates a `yetii.yaml` config file with default structure and settings.

---

### ✅ `odbc` — Check Installed ODBC Drivers

Lists all ODBC drivers detected on your system to ensure database connectivity prerequisites are met.

**Usage:**

```bash
yetii odbc
```

**Output:**
- Lists all available ODBC drivers
- Useful for confirming database drivers before running queries

### ✅ `setup` — Install ODBC Prerequisites

Reads the configured database types and drivers from YAML, then installs the supported system packages. For multi-database configs, duplicate driver work is skipped.

- macOS: Homebrew `unixodbc` and `psqlodbc`, including driver registration
- Ubuntu/Debian: `unixodbc` and `odbc-postgresql`
- Fedora/RHEL, openSUSE, Arch, and Alpine: corresponding unixODBC and PostgreSQL driver packages
- Windows: PostgreSQL or Microsoft SQL Server drivers through WinGet

```bash
# Preview system changes
yetii --file yetii.yaml setup --dry-run

# Install missing prerequisites
yetii --file yetii.yaml setup
```

PostgreSQL is automated across macOS and Linux. Windows also automates SQL Server Driver 18. Custom drivers, MySQL packages that vary by distribution, and Oracle Instant Client produce explicit manual-install guidance instead of installing an incompatible package.

---

### ✅ `run` — Execute Queries

Runs the Yetii application with configured queries and operations.

**Usage:**

```bash
yetii run [OPTIONS]
```

**Options:**
* `--query, -q <QUERY>`: (Optional) Name of a specific query to run
* `--force, -f`: (Optional) Force execution even if query is disabled in configuration

**Current behavior:**

- Selects one query with `--query`, or all enabled queries
- Executes SQL through ODBC with safe bound parameters for `$name`, `:name`, or numeric positional parameter keys
- Supports multiple configured databases and reuses one ODBC connection per database group in a run
- Converts common ODBC result types to native JSON numbers, booleans, strings, and null values
- Applies filters, simple type conversions, and field mappings before delivery
- Sends rows in configured batches with endpoint headers and Bearer, API-key, Basic, or OAuth2 client-credentials authentication
- Retries transient HTTP failures using configured fixed or exponential backoff
- Resolves incremental parameters from a JSON state file and advances state only after successful delivery
- Validates configured HTTP success status codes
- Emits structured JSON logs and a `rows_read`, `batches_sent`, and `failures` summary

---

### ✅ `daemon` — Run Scheduled Queries

Runs enabled queries that have an enabled `schedule` block. Cron expressions may use either five fields (`*/5 * * * *`) or six fields with seconds (`*/10 * * * * *`).

**Foreground:**

```bash
yetii --file yetii.yaml daemon start
```

**Detached/background:**

```bash
yetii --file yetii.yaml daemon start --detach \
  --pid-file .yetii/yetii.pid \
  --log-file .yetii/yetii.log
```

**Status and stop:**

```bash
yetii daemon status --pid-file .yetii/yetii.pid
yetii daemon stop --pid-file .yetii/yetii.pid
```

The daemon uses `execution.scheduler.max_concurrent_jobs` as a concurrency limit. Currently `missed_job_policy` must be `skip`.

---

### ✅ `check-config` — Validate Configuration

Validates the Yetii YAML configuration file for correctness and completeness.

**Usage:**

```bash
yetii check-config
```

**Output:**
- ✅ Success message if configuration is valid
- ❌ Detailed error messages if configuration issues are found

---

## 🔧 Installation & Usage

### Prerequisites

- Rust toolchain installed
- ODBC driver manager and database driver, installable with `yetii setup` for supported platforms

### Build from Source

```bash
git clone <repository-url>
cd yetii
cargo build --release
```

### Global Configuration

Yetii uses a global configuration file specified via the `--file` flag:

```bash
yetii --file custom-config.yaml <COMMAND>
```

**Default:** `yetii.yaml` in the current directory

### Example Workflow

```bash
# 1. Initialize a new Yetii project
yetii init --path ./my-project

# 2. Install and verify the YAML-selected ODBC driver
yetii setup
yetii odbc

# 3. Validate your configuration
yetii check-config

# 4. Run all configured queries
yetii run

# 5. Run specific query with force flag
yetii run --query my_query --force
```

---

## 📋 Configuration File Structure

The `yetii.yaml` configuration file structure includes:

```yaml
version: "0.0.1"
name: "yetii.config"
databases:
  - name: main
    type: postgres
    # Optional override; otherwise Yetii uses the database-type default.
    driver: PostgreSQL Unicode
    host: localhost
    port: 5432
    database: postgres
    connection_options:
      SSLmode: require
    auth:
      username: sync_user
      password: ${YETII_DB_PASSWORD}

  - name: billing
    type: mssql
    driver: ODBC Driver 18 for SQL Server
    host: billing-db.example.com
    port: 1433
    database: billing
    connection_options:
      Encrypt: yes
      TrustServerCertificate: no
    auth:
      username: billing_sync
      password: ${BILLING_DB_PASSWORD}

queries:
  - name: health_sync
    description: Example query
    enabled: true
    database: main
    schedule:
      cron: "*/5 * * * *"
      timezone: UTC
      enabled: true
    query:
      sql: SELECT current_database() AS database_name WHERE current_user = :db_user
      parameters:
        db_user:
          type: string
          default: sync_user
    transform:
      filters:
        - field: database_name
          condition: not_null
      conversions:
        database_name:
          from: string
          to: string
      mappings:
        database_name: source_database
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

execution:
  state_management:
    enabled: true
    state_file: ./state/yetii_state.json
    backup_states: 5
  scheduler:
    enabled: true
    max_concurrent_jobs: 2
    job_timeout_minutes: 30
    missed_job_policy: skip
```

Configuration validation ensures:
- Required fields are present
- Data types are correct
- Query definitions are properly structured
- Database connections are configured

`${ENV_VAR}` references in YAML values are resolved when Yetii loads the config. Use `connection_options` for cloud or driver-specific ODBC attributes such as `SSLmode=require`, `Encrypt=yes`, or `TrustServerCertificate=no`.

The older single-object `databases:` shape is still accepted for one-database configs. When multiple databases are configured, each query must set `database`.

### Incremental state

When `execution.state_management.enabled` is true, Yetii reads and writes a JSON state file. A query parameter with `source: state_file` is resolved from that query's saved watermark; if no saved value exists, Yetii uses the parameter `default`.

```yaml
query:
  sql: |
    SELECT *
    FROM orders
    WHERE updated_at > $last_run_time
    ORDER BY updated_at
  parameters:
    last_run_time:
      type: timestamp
      source: state_file
      default: "1970-01-01T00:00:00Z"
```

State is advanced only after the query rows are transformed and all HTTP batches are delivered successfully. Before each write, Yetii rotates backups such as `yetii_state.json.1`, up to `backup_states`.

---

## 🏗️ Architecture

Yetii is built with a modular architecture:

```
yetii/
├── Cargo.toml              # Project dependencies and metadata
├── Cargo.lock              # Dependency lock file
├── README.md               # Project documentation
├── logo.png                # Project logo
└── src/
    ├── main.rs             # Application entry point
    ├── cli/                # CLI argument parsing and command definitions
    │   └── mod.rs
    ├── commands/           # Command implementations
    │   ├── mod.rs
    │   ├── initialize.rs   # Config initialization logic
    │   ├── odbc.rs         # ODBC driver checking
    │   └── run.rs          # Query execution logic
    ├── config/             # Configuration management
    ├── database/           # ODBC connectivity, parameters, and typed extraction
    ├── http/               # HTTP delivery and retry logic
    ├── state/              # Incremental run state and backup rotation
    └── transform/          # Row filtering, conversions, and mappings
```

**Key Components:**
- **CLI Module**: Command-line interface built with [`clap`](https://docs.rs/clap/)
- **Commands Module**: Individual command implementations (init, odbc, run, check-config)
- **Config Module**: YAML configuration management with [`serde_yaml`](https://docs.rs/serde_yaml/)
- **Database Module**: ODBC connectivity with connection-string generation, bound parameters, typed extraction, and run-scoped connection reuse
- **HTTP Module**: Batched JSON delivery with authentication, OAuth2 token management, success-code validation, and retry/backoff handling
- **State Module**: JSON state-file loading, backup rotation, and incremental parameter watermarks
- **Transform Module**: Row-level filters, simple type conversions, and field mappings
- **ODBC Integration**: System ODBC driver detection and validation

---

## 📅 Roadmap

### Current Status
* [x] CLI interface with clap
* [x] Config file initialization
* [x] ODBC environment checking
* [x] Configuration validation
* [x] YAML-aware ODBC prerequisite setup
* [x] ODBC query execution
* [x] Batched HTTP JSON delivery
* [x] Structured run reporting
* [x] `${ENV_VAR}` config interpolation
* [x] Cloud-friendly ODBC `connection_options`
* [x] Bound parameter execution
* [x] Retry and backoff policies
* [x] Typed result extraction
* [x] Run-scoped ODBC connection reuse
* [x] OAuth2 client-credentials HTTP auth
* [x] Transform stage with filters, conversions, and field mappings
* [x] Multi-database configuration and per-database query grouping
* [x] Scheduler daemon with foreground and detached modes
* [x] State-file backed incremental parameters

### Upcoming Features
* [ ] Full connection pooling and concurrent worker model
* [ ] Advanced transforms such as grouping and aggregation
* [ ] Configuration schema documentation
* [ ] Configuration hot reload for daemon mode

---

## 🧪 Development

### Running Commands in Development

```bash
# Run with cargo
cargo run -- init --path .
cargo run -- odbc
cargo run -- run --query my_query
cargo run -- check-config

# With custom config file
cargo run -- --file custom.yaml check-config
```

### Testing

```bash
cargo test
```

---

## 🐛 Error Handling

Yetii provides clear error messages for common issues:

- **Configuration errors**: Detailed validation messages with line numbers
- **ODBC issues**: Clear driver availability reporting
- **File system errors**: Helpful messages for path and permission issues
- **Command errors**: Usage hints and suggestions

---

## 🤝 Contributing

Currently an internal project. Future contributions welcome for:
- Additional database driver support
- Query optimization features
- Configuration schema enhancements
- Documentation improvements

---

## 📄 License

MIT © 2025 Emmanuel Muthiani

---

## 🆘 Support

For issues or questions:
1. Check the configuration validation output: `yetii check-config`
2. Verify ODBC drivers: `yetii odbc`
3. Review the generated config file structure
4. Ensure proper file permissions for config directory
