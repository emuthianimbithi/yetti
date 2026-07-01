use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Yetii {
    #[arg(global = true, long, short = 'c', default_value = "yetii.yaml")]
    pub file: String,
    #[arg(global = true, long, short = 'v', action = clap::ArgAction::SetTrue)]
    pub verbose: bool,
    #[clap(subcommand)]
    pub commands: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a starter Yetii configuration.
    Init {
        /// Directory where the starter configuration should be written.
        #[clap(short, long, default_value = ".")]
        path: String,
    },
    /// List registered ODBC drivers.
    #[clap(name = "odbc")]
    CheckExistingOdbc,

    /// Install and configure system prerequisites required by the YAML database configuration.
    #[clap(name = "setup")]
    Setup {
        /// Print the required changes without installing or registering anything.
        #[clap(long)]
        dry_run: bool,
    },

    /// Execute configured queries and deliver their rows to HTTP endpoints.
    #[clap(name = "run")]
    Run {
        /// Name of one query to run. Runs all enabled queries when omitted.
        #[clap(short, long)]
        query: Option<String>,

        /// Run disabled queries too.
        #[clap(short, long)]
        force: bool,
    },

    /// Validate the Yetii configuration.
    #[clap(name = "check-config")]
    CheckConfig,

    /// Run scheduled queries continuously.
    #[clap(name = "daemon")]
    Daemon {
        #[clap(subcommand)]
        command: DaemonCommand,
    },
}

#[derive(Subcommand)]
pub enum DaemonCommand {
    /// Start the scheduler daemon.
    Start {
        /// Start in the background and return immediately.
        #[clap(long)]
        detach: bool,

        /// PID file used by detached/status/stop.
        #[clap(long, default_value = ".yetii/yetii.pid")]
        pid_file: String,

        /// Log file used when starting detached.
        #[clap(long, default_value = ".yetii/yetii.log")]
        log_file: String,
    },

    /// Report whether the daemon PID is running.
    Status {
        /// PID file to inspect.
        #[clap(long, default_value = ".yetii/yetii.pid")]
        pid_file: String,
    },

    /// Stop the daemon process recorded in the PID file.
    Stop {
        /// PID file to inspect and remove.
        #[clap(long, default_value = ".yetii/yetii.pid")]
        pid_file: String,
    },
}
