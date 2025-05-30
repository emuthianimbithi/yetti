use clap::{Parser,Subcommand};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Yetii {
    #[clap(subcommand)]
    pub commands: Commands,
}

#[derive(Subcommand)]
/// Yetii CLI commands
/// This module defines the commands available in the Yetii CLI.
/// Each command is documented with its purpose, usage examples, and expected outcomes.
/// # Commands:
/// - `init`: Initializes the Yetii application.
/// - `odbc`: Checks for existing ODBC drivers on the system.
/// # Example usage:
/// ```
/// yetii init --config my_config.config
/// yetii odbc
/// This module provides a structured way to manage Yetii's functionality through the command line interface.
pub enum Commands{
    /// Initialize Yetii
    /// This command initializes the Yetii application, setting up the necessary configuration files and directories.
    /// # Example usage:
    ///```
    /// yetii init
    /// ```
    ///This command is useful for setting up Yetii for the first time or resetting its configuration.
    ///# Returns:
    ///- A success message if the initialization is successful.
    ///- An error message if the initialization fails.
    Init {
        /// path to the configuration file
        /// This option allows you to specify the path where the configuration file will be created.
        /// # Example usage:
        /// ```
        /// yetii init --path /path/to/config
        /// ```
        /// This is useful for organizing configurations in a specific directory or for using a custom path.
        /// # Returns:
        /// - A success message if the initialization is successful.
        /// - An error message if the initialization fails.
        #[clap(short, long, default_value = ".")]
        path: String,
    },
    /// Check if ODBC drivers are installed
    /// This command checks for existing ODBC drivers on the system.
    /// It will return a list of installed ODBC drivers or an error if the check fails.
    /// # Example usage:
    /// ```
    /// yetii odbc
    /// ```
    ///This command is useful for ensuring that the necessary ODBC drivers are available before proceeding with database operations.
    /// # Returns:
    /// - A list of installed ODBC drivers if the check is successful.
    ///- An error message if the check fails.
    #[clap(name = "odbc")]
    CheckExistingOdbc,
    /// Run Yetii
    /// This command runs the Yetii application, executing the configured queries and operations.
    /// # Example usage:
    ///  ```
    /// yetii run
    /// This command is useful for executing the main functionality of Yetii after it has been initialized and configured.
    /// # Returns:
    /// - A success message if the application runs successfully.
    /// - An error message if the application fails to run.

    #[clap(name = "run")]
    Run{
        /// name of the query to run
        /// This option allows you to specify a specific query to run.
        /// # Example usage:
        /// ```
        /// yetii run --query my_query
        /// ```
        /// This is useful for executing a specific query without running all configured queries.
        #[clap(short, long)]
        query: Option<String>,

        /// force the execution of the query even if it is not enabled
        /// This flag allows you to force the execution of a query even if it is marked as disabled in the configuration.
        /// # Example usage:
        /// ```
        /// yetii run --force
        /// ```
        #[clap(short, long)]
        force: Option<bool>,
    },
    /// Check Yetii configuration
    /// This command checks the Yetii configuration for validity and completeness.
    /// It will validate the configuration file and ensure that all required settings are present.
    /// # Example usage:
    /// ```
    /// yetii check-config
    /// ```
    /// This command is useful for verifying that the Yetii configuration is set up correctly before running any operations.
    /// # Returns:
    /// - A success message if the configuration is valid.
    /// - An error message if the configuration is invalid or incomplete.
    #[clap(name = "check-config")]
    CheckConfig,
}

