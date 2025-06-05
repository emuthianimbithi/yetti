use std::error::Error;
use crate::commands::odbc;
use crate::config;

pub fn run() -> Result<String, Box<dyn Error>> {
    // Check if ODBC drivers are installed
    if let Err(e) = odbc::check_odbc_drivers() {
        eprintln!("Error checking ODBC drivers: {}", e);
        return Err(e);
    }
    println!("ODBC Drivers found.");

    // Load the Yetii configuration
    let cfg = config::get_config().map_err(|e| {
        eprintln!("Error accessing configuration: {}", e);
        e
    })?;

    // validate the configuration
    config::validate_config(&cfg).map_err(|e| {
        eprintln!("Error validating Yetii configuration: {}", e);
        e
    })?;

    println!("✅ Yetii configuration file is valid.");

    Ok("Configuration validated successfully".to_string())
}