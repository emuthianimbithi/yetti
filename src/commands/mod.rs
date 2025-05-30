mod initialize;
mod odbc;
mod run;

use crate::cli::{Commands, Yetii};
use crate::{config};
use crate::config::CONFIG;

pub fn going_through_commands(yetii: &Yetii){
    match &yetii.commands {
        Commands::Init{ path} => {
            match initialize::initialize_yetii_config("", path) {
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
           match odbc::check_odbc_drivers(){
                Ok(output) => println!("ODBC Drivers found:\n{}", output),
                Err(e) => eprintln!("Error checking ODBC drivers: {}", e),
            }
            match config::YetiiConfig::validate(&CONFIG.read().expect("CONFIG lock poisoned")) {
                Ok(_) => println!("Yetii configuration is valid."),
                Err(e) => eprintln!("Error validating Yetii configuration: {}", e),
            }
        }
        Commands::CheckConfig=> {
            match config::YetiiConfig::validate(&CONFIG.read().expect("CONFIG lock poisoned")) {
                Ok(_) => println!("✅ Yetii configuration file is valid."),
                Err(e) => eprintln!("❌❌Error validating Yetii configuration file: {}❌❌", e),
            }
        }
    }
}