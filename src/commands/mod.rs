mod initialize;
mod odbc;
mod run;
use crate::cli::{Commands, Yetii};
use crate::{config};
pub fn going_through_commands(yetii: &Yetii){
// This function processes the commands provided by the user through the Yetii CLI.
// It matches the command and executes the corresponding functionality.
// Each command has its own logic and can interact with the Yetii application in different ways.
    // first init the config file

    match &yetii.commands {
        Commands::Init{ path} => {
            let config_name = std::path::Path::new(&yetii.file)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("yetii.yaml");
            match initialize::initialize_yetii_config(config_name, path) {
                Ok(message) => println!("{}", message),
                Err(e) => eprintln!("Error initializing Yetii configuration: {}", e),
            }
        }
        Commands::CheckExistingOdbc => {
            match odbc::check_odbc_drivers(){
                Ok(output) => println!("ODBC Drivers found:\n{}", output),
                Err(e) => eprintln!("Error checking ODBC drivers: {}", e),
            }
        }
        Commands::Run { query: _query,force: _force }=> {
            match run::run() {
                Ok(output) => println!("Run completed successfully:\n{}", output),
                Err(e) => eprintln!("Error running Yetii: {}", e),
            }
        }
        Commands::CheckConfig=> {
            match config::get_config() {
                Ok(cfg) => {
                    match config::yetii::YetiiConfig::validate(&cfg) {
                        Ok(_) => println!("✅ Yetii configuration file is valid."),
                        Err(e) => eprintln!("❌❌Error validating Yetii configuration file: {}❌❌", e),
                    }
                }
                Err(e) => eprintln!("Error accessing configuration: {}", e),
            }
        }
    }
}