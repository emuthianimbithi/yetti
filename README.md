Got it! Here’s an updated README draft for your **YETII** CLI project incorporating the info you shared, polished for clarity, formatting, and completeness:

---

<p align="center">
  <img src="logo.png" alt="YETII Logo" width="500" style="border-radius: 20px;" />
</p>

# 🧊 Yetii (YAML Enterprise Transformation & Integration Interface) CLI

**Yetii** is a Rust-based CLI tool designed to streamline ERP integration through flexible YAML-based configuration management. It helps developers initialize configuration files, verify ODBC drivers on the system, and eventually will support executing parameterized queries and API endpoints.

> ⚠️ Work-in-progress — expect evolving features and improvements.

---

## 🚀 Current Features

### ✅ `init` — Initialize Configuration

Create a starter YAML config file with default values for Yetii.

**Usage:**

```bash
yetii init --path .
```

* `--path, -p`: Directory where the config file is created (default: current directory).

This generates a config file (e.g., `yetii.config`) with content like:

```yaml
version: "0.0.1"
name: "yetii.config"
# ...
```

---

### ✅ `odbc` — Check Installed ODBC Drivers

Lists all ODBC drivers detected on your system.

**Usage:**

```bash
yetii odbc
```

Useful for confirming available database drivers before running queries.

---

### ✅ `run` — Run Queries

Executes queries as configured (placeholder implementation currently).

**Usage:**

```bash
yetii run --query my_query --force
```

* `--query, -q`: (Optional) Name of a specific query to run.
* `--force, -f`: (Optional) Force execution even if query is disabled.

Currently this also runs an ODBC check and config validation.

---

### ✅ `check-config` — Validate Configuration

Verifies the Yetii YAML config file for correctness.

**Usage:**

```bash
yetii check-config
```

---

## 🔧 Build and Run

### Build

```bash
cargo build
```

### Run CLI commands

```bash
cargo run -- <COMMAND>
```

Examples:

```bash
cargo run -- init --path .
cargo run -- odbc
cargo run -- run --query my_query
cargo run -- check-config
```

---

## 📅 Roadmap

* [x] Config initialization
* [x] ODBC environment check
* [ ] Load & parse full YAML config
* [ ] Execute parameterized SQL queries
* [ ] API endpoint integration (POST/PUT)
* [ ] Error handling & monitoring
* [ ] Config validation & schema enforcement

---

## 📄 License

MIT © 2025 Emmanuel Muthiani

---

## 🧪 Development Notes

Uses:

* [`clap`](https://docs.rs/clap/latest/clap/) for CLI argument parsing
* [`serde_yaml`](https://docs.rs/serde_yaml/latest/serde_yaml/) for YAML config serialization/deserialization

```rust
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
```

---

## 🤝 Contributing

Currently an internal project; feedback and contributions welcome in future updates.

---

If you want, I can also help you generate a full markdown file or a GitHub-flavored README with badges, usage examples, or more detailed instructions! Would you like that?
