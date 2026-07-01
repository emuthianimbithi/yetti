use std::io;
use std::process::Command;

pub fn check_odbc_drivers() -> anyhow::Result<String> {
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
                    anyhow::bail!("PowerShell command failed: {}", err)
                }
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    anyhow::bail!("PowerShell not found on this system")
                } else {
                    Err(e.into())
                }
            }
        };
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let output = Command::new("odbcinst").arg("-j").output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    let result = String::from_utf8_lossy(&output.stdout).to_string();
                    Ok(result)
                } else {
                    let err = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("`odbcinst` command failed: {}", err)
                }
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    anyhow::bail!("`odbcinst` not found. Please install unixODBC.")
                } else {
                    Err(e.into())
                }
            }
        }
    }
}
