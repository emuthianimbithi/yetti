use std::error::Error;
use std::io;
use std::process::Command;

pub fn check_odbc_drivers() -> Result<String, Box<dyn Error>> {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("powershell")
            .args(&["-Command", "Get-OdbcDriver | Select-Object Name"])
            .output();

        return match output {
            Ok(output) => {
                if output.status.success() {
                    let result = String::from_utf8_lossy(&output.stdout).to_string();
                    Ok(result)
                } else {
                    let err = String::from_utf8_lossy(&output.stderr);
                    Err(format!("PowerShell command failed: {}", err).into())
                }
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    Err("PowerShell not found on this system".into())
                } else {
                    Err(e.into())
                }
            }
        };
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let output = Command::new("odbcinst")
            .arg("-j")
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    let result = String::from_utf8_lossy(&output.stdout).to_string();
                    Ok(result)
                } else {
                    let err = String::from_utf8_lossy(&output.stderr);
                    Err(format!("`odbcinst` command failed: {}", err).into())
                }
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    Err("`odbcinst` not found. Please install unixODBC.".into())
                } else {
                    Err(e.into())
                }
            }
        }
    }
}