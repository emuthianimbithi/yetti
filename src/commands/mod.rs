mod initialize;

use crate::cli::{Commands, Yetii};
use crate::utils;
pub fn going_through_commands(yetii: &Yetii){
    match &yetii.commands {
        Commands::Init {config, path} => {
            match initialize::initialize_yetii_config(config, path) {
                Ok(message) => println!("{}", message),
                Err(e) => eprintln!("Error initializing Yetii configuration: {}", e),
            }
        }
        Commands::CheckExistingOdbc => {
            match utils::odbc_check::check_odbc_drivers(){
                Ok(output) => println!("ODBC Drivers found:\n{}", output),
                Err(e) => eprintln!("Error checking ODBC drivers: {}", e),
            }
        }
    }
}