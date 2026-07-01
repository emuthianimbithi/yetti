use crate::config::database::{DatabaseConfig, DatabaseConfigs, DatabaseType};
use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::fmt;
#[cfg(target_os = "macos")]
use std::fs::OpenOptions;
#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
#[cfg(target_os = "macos")]
use std::process::Output;

#[derive(Debug)]
pub struct SetupReport {
    actions: Vec<String>,
}

impl fmt::Display for SetupReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.actions.is_empty() {
            return write!(formatter, "ODBC prerequisites are ready");
        }
        write!(formatter, "{}", self.actions.join("\n"))
    }
}

pub async fn run(databases: &DatabaseConfigs, dry_run: bool) -> Result<SetupReport> {
    let databases = databases.clone();
    tokio::task::spawn_blocking(move || run_all_blocking(&databases, dry_run))
        .await
        .context("setup worker failed")?
}

fn run_all_blocking(databases: &DatabaseConfigs, dry_run: bool) -> Result<SetupReport> {
    let mut actions = Vec::new();
    let mut seen = HashSet::new();

    for database in databases.as_slice() {
        let key = (
            database_type_name(&database.db_type).to_string(),
            requested_driver(database).to_string(),
        );
        if !seen.insert(key) {
            continue;
        }

        let report = run_blocking(database, dry_run)
            .with_context(|| format!("setup for database '{}' failed", database.name))?;
        actions.extend(report.actions);
    }

    actions.sort();
    actions.dedup();
    Ok(SetupReport { actions })
}

fn run_blocking(database: &DatabaseConfig, dry_run: bool) -> Result<SetupReport> {
    if database.connection_string.is_some() && database.driver.is_none() {
        bail!(
            "automatic setup cannot infer a driver from database.connection_string; also set databases.driver or install its referenced ODBC driver manually"
        );
    }

    #[cfg(target_os = "macos")]
    {
        setup_macos(database, dry_run)
    }

    #[cfg(target_os = "linux")]
    {
        setup_linux(database, dry_run)
    }

    #[cfg(target_os = "windows")]
    {
        setup_windows(database, dry_run)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        bail!(
            "automatic setup is not supported on this operating system; install an ODBC driver manager and '{}' manually",
            database
                .driver
                .as_deref()
                .unwrap_or_else(|| database.db_type.default_odbc_driver())
        )
    }
}

#[cfg(target_os = "linux")]
fn setup_linux(database: &DatabaseConfig, dry_run: bool) -> Result<SetupReport> {
    ensure_automatic_postgres_driver(database)?;
    let package_manager = detect_linux_package_manager().context(
        "no supported Linux package manager found; supported managers: apt-get, dnf, yum, zypper, pacman, apk",
    )?;
    let (manager, packages, install_args): (&str, &[&str], &[&str]) = match package_manager.as_str()
    {
        "apt-get" => (
            "apt-get",
            &["unixodbc", "odbc-postgresql"],
            &["install", "-y"],
        ),
        "dnf" => ("dnf", &["unixODBC", "postgresql-odbc"], &["install", "-y"]),
        "yum" => ("yum", &["unixODBC", "postgresql-odbc"], &["install", "-y"]),
        "zypper" => (
            "zypper",
            &["unixODBC", "psqlODBC"],
            &["--non-interactive", "install"],
        ),
        "pacman" => (
            "pacman",
            &["unixodbc", "psqlodbc"],
            &["-S", "--needed", "--noconfirm"],
        ),
        "apk" => ("apk", &["unixodbc", "psqlodbc"], &["add"]),
        _ => unreachable!(),
    };

    let mut arguments = install_args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    arguments.extend(packages.iter().map(|value| value.to_string()));
    let action = format!("install with {manager}: {}", packages.join(" "));
    if dry_run {
        return Ok(SetupReport {
            actions: vec![action],
        });
    }

    if manager == "apt-get" {
        run_privileged(manager, &["update".to_string()])?;
    }
    run_privileged(manager, &arguments)?;
    let requested_driver = requested_driver(database);
    if !installed_drivers()?
        .iter()
        .any(|name| name == requested_driver)
    {
        bail!(
            "packages installed, but ODBC driver '{}' is not registered; inspect `odbcinst -q -d` and set databases.driver to the registered name",
            requested_driver
        );
    }

    Ok(SetupReport {
        actions: vec![action],
    })
}

#[cfg(target_os = "linux")]
fn detect_linux_package_manager() -> Option<String> {
    ["apt-get", "dnf", "yum", "zypper", "pacman", "apk"]
        .into_iter()
        .find(|manager| find_command(&[manager]).is_some())
        .map(str::to_string)
}

#[cfg(target_os = "linux")]
fn run_privileged(program: &str, arguments: &[String]) -> Result<()> {
    let root = Command::new("id")
        .arg("-u")
        .output()
        .is_ok_and(|output| output.status.success() && output.stdout == b"0\n");
    let status = if root {
        Command::new(program).args(arguments).status()
    } else {
        Command::new("sudo").arg(program).args(arguments).status()
    }
    .with_context(|| format!("failed to start {program}"))?;

    if !status.success() {
        bail!("{program} failed while installing ODBC prerequisites");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn setup_windows(database: &DatabaseConfig, dry_run: bool) -> Result<SetupReport> {
    let requested_driver = requested_driver(database);
    if installed_windows_drivers()?
        .iter()
        .any(|name| name == requested_driver)
    {
        return Ok(SetupReport {
            actions: Vec::new(),
        });
    }

    let package_id = match database.db_type {
        DatabaseType::Postgres if is_supported_postgres_driver(requested_driver) => {
            "PostgreSQL.psqlODBC"
        }
        DatabaseType::Mssql if requested_driver == "ODBC Driver 18 for SQL Server" => {
            "Microsoft.msodbcsql.18"
        }
        _ => {
            bail!(
                "automatic Windows setup for '{}' driver '{}' is not available; install the vendor's 64-bit ODBC driver manually",
                database_type_name(&database.db_type),
                requested_driver
            )
        }
    };
    find_command(&["winget"]).context(
        "Windows Package Manager (winget) is required; install Microsoft App Installer first",
    )?;
    let action = format!("install winget package '{package_id}'");
    if dry_run {
        return Ok(SetupReport {
            actions: vec![action],
        });
    }

    let status = Command::new("winget")
        .args([
            "install",
            "--exact",
            "--id",
            package_id,
            "--accept-package-agreements",
            "--accept-source-agreements",
        ])
        .status()
        .context("failed to start winget")?;
    if !status.success() {
        bail!("winget failed to install '{package_id}'");
    }
    if !installed_windows_drivers()?
        .iter()
        .any(|name| name == requested_driver)
    {
        bail!(
            "winget installed '{package_id}', but ODBC driver '{}' is still unavailable",
            requested_driver
        );
    }
    Ok(SetupReport {
        actions: vec![action],
    })
}

#[cfg(target_os = "windows")]
fn installed_windows_drivers() -> Result<Vec<String>> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-OdbcDriver | Select-Object -ExpandProperty Name",
        ])
        .output()
        .context("failed to query Windows ODBC drivers with PowerShell")?;
    if !output.status.success() {
        bail!(
            "PowerShell failed to query ODBC drivers: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

#[cfg(target_os = "macos")]
fn setup_macos(database: &DatabaseConfig, dry_run: bool) -> Result<SetupReport> {
    ensure_automatic_postgres_driver(database)?;
    let requested_driver = requested_driver(database);

    let brew = find_command(&["/opt/homebrew/bin/brew", "/usr/local/bin/brew", "brew"])
        .context("Homebrew is required for automatic setup; install it from https://brew.sh")?;
    let mut actions = Vec::new();
    ensure_formula(&brew, "unixodbc", dry_run, &mut actions)?;
    ensure_formula(&brew, "psqlodbc", dry_run, &mut actions)?;

    if installed_drivers()?
        .iter()
        .any(|name| name == requested_driver)
    {
        return Ok(SetupReport { actions });
    }

    actions.push(format!("register ODBC driver '{requested_driver}'"));
    if !dry_run {
        let prefix = command_output(&brew, &["--prefix", "psqlodbc"])?;
        register_postgres_drivers(Path::new(prefix.trim()))?;
        if !installed_drivers()?
            .iter()
            .any(|name| name == requested_driver)
        {
            bail!(
                "ODBC driver registration completed but '{requested_driver}' is still unavailable"
            );
        }
    }

    Ok(SetupReport { actions })
}

#[cfg(target_os = "macos")]
fn ensure_formula(
    brew: &Path,
    formula: &str,
    dry_run: bool,
    actions: &mut Vec<String>,
) -> Result<()> {
    let installed = Command::new(brew)
        .args(["list", "--versions", formula])
        .output()
        .with_context(|| format!("failed to check Homebrew formula '{formula}'"))?
        .status
        .success();
    if installed {
        return Ok(());
    }

    actions.push(format!("install Homebrew formula '{formula}'"));
    if !dry_run {
        let status = Command::new(brew)
            .args(["install", formula])
            .status()
            .with_context(|| format!("failed to start Homebrew installation for '{formula}'"))?;
        if !status.success() {
            bail!("Homebrew failed to install '{formula}'");
        }
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn installed_drivers() -> Result<Vec<String>> {
    let output = Command::new("odbcinst")
        .args(["-q", "-d"])
        .output()
        .context("failed to query registered ODBC drivers; is unixODBC installed?")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().trim_start_matches('[').trim_end_matches(']'))
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

#[cfg(target_os = "macos")]
fn register_postgres_drivers(prefix: &Path) -> Result<()> {
    let unicode_driver = prefix.join("lib/psqlodbcw.so");
    let ansi_driver = prefix.join("lib/psqlodbca.so");
    if !unicode_driver.exists() || !ansi_driver.exists() {
        bail!(
            "psqlodbc libraries were not found under '{}'",
            prefix.display()
        );
    }

    let template_path = setup_template_path();
    let mut template = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&template_path)
        .with_context(|| {
            format!(
                "failed to create temporary ODBC template '{}'",
                template_path.display()
            )
        })?;
    writeln!(
        template,
        "[PostgreSQL Unicode]\nDescription=PostgreSQL ODBC Unicode driver\nDriver={}\nSetup={}\n\n[PostgreSQL ANSI]\nDescription=PostgreSQL ODBC ANSI driver\nDriver={}\nSetup={}",
        unicode_driver.display(),
        unicode_driver.display(),
        ansi_driver.display(),
        ansi_driver.display()
    )?;
    drop(template);

    let result = Command::new("odbcinst")
        .args(["-i", "-d", "-f"])
        .arg(&template_path)
        .output()
        .context("failed to register PostgreSQL ODBC drivers");
    let _ = std::fs::remove_file(&template_path);
    let output = result?;
    if !output.status.success() {
        bail!(
            "odbcinst failed to register PostgreSQL drivers: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn setup_template_path() -> PathBuf {
    std::env::temp_dir().join(format!("yetii-odbc-{}.ini", std::process::id()))
}

#[cfg(target_os = "macos")]
fn command_output(command: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(command)
        .args(args)
        .output()
        .with_context(|| format!("failed to run '{}'", command.display()))?;
    checked_stdout(output, command)
}

#[cfg(target_os = "macos")]
fn checked_stdout(output: Output, command: &Path) -> Result<String> {
    if !output.status.success() {
        bail!(
            "'{}' failed: {}",
            command.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn find_command(candidates: &[&str]) -> Option<PathBuf> {
    candidates.iter().map(PathBuf::from).find(|candidate| {
        candidate.is_absolute() && candidate.exists()
            || Command::new(candidate)
                .arg("--version")
                .output()
                .is_ok_and(|output| output.status.success())
    })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn ensure_automatic_postgres_driver(database: &DatabaseConfig) -> Result<()> {
    if !matches!(database.db_type, DatabaseType::Postgres) {
        bail!(
            "automatic setup for '{}' is not implemented on this platform; install the '{}' ODBC driver manually",
            database_type_name(&database.db_type),
            requested_driver(database)
        );
    }
    if !is_supported_postgres_driver(requested_driver(database)) {
        bail!(
            "custom ODBC driver '{}' cannot be installed automatically; install and register it manually",
            requested_driver(database)
        );
    }
    Ok(())
}

fn requested_driver(database: &DatabaseConfig) -> &str {
    database
        .driver
        .as_deref()
        .unwrap_or_else(|| database.db_type.default_odbc_driver())
}

fn is_supported_postgres_driver(driver: &str) -> bool {
    matches!(driver, "PostgreSQL Unicode" | "PostgreSQL ANSI")
}

fn database_type_name(database_type: &DatabaseType) -> &'static str {
    match database_type {
        DatabaseType::Postgres => "postgres",
        DatabaseType::Mysql => "mysql",
        DatabaseType::Mssql => "mssql",
        DatabaseType::Oracle => "oracle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_homebrew_postgres_drivers_are_automatic() {
        assert!(is_supported_postgres_driver("PostgreSQL Unicode"));
        assert!(is_supported_postgres_driver("PostgreSQL ANSI"));
        assert!(!is_supported_postgres_driver("Company PostgreSQL Driver"));
    }
}
