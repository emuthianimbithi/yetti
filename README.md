<p align="center">
  <img src="logo.png" alt="YETII Logo" width="500" style="border-radius: 20px;" />
</p>

# 🧊 Yetii (YAML Enterprise Transformation & Integration Interface) CLI

**Yetii** is a Rust-based CLI tool designed to streamline ERP integration through flexible YAML-based configuration management. It helps developers initialize configuration files, verify ODBC drivers on the system, validate configurations, and execute parameterized queries.

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
- Performs ODBC driver check
- Validates configuration file
- Prepares for query execution (implementation in progress)

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
- ODBC drivers for your target databases (optional, for database operations)

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

# 2. Check available ODBC drivers
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
# Additional configuration options...
```

Configuration validation ensures:
- Required fields are present
- Data types are correct
- Query definitions are properly structured
- Database connections are configured

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
    │   └── mod.rs
    └── database/           # Database connectivity layer
        ├── mod.rs
        └── postgres.rs     # PostgreSQL specific implementation
```

**Key Components:**
- **CLI Module**: Command-line interface built with [`clap`](https://docs.rs/clap/)
- **Commands Module**: Individual command implementations (init, odbc, run, check-config)
- **Config Module**: YAML configuration management with [`serde_yaml`](https://docs.rs/serde_yaml/)
- **Database Module**: Database connectivity layer with PostgreSQL support
- **ODBC Integration**: System ODBC driver detection and validation

---

## 📅 Roadmap

### Current Status
* [x] CLI interface with clap
* [x] Config file initialization
* [x] ODBC environment checking
* [x] Configuration validation
* [x] Command structure foundation

### Upcoming Features
* [ ] Full YAML config parsing and loading
* [ ] PostgreSQL query execution via dedicated database module
* [ ] Parameterized SQL query execution
* [ ] Multi-database support (expanding from PostgreSQL base)
* [ ] Database connection pooling and management
* [ ] API endpoint integration (POST/PUT)
* [ ] Query result processing and transformation
* [ ] Error handling and logging improvements
* [ ] Configuration schema documentation
* [ ] Test suite expansion

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