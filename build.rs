//! Build script: downloads vendor SDK libraries when the `download-sdk` feature is enabled.

use std::path::Path;
use std::process::Command;

fn main() {
    if std::env::var("CARGO_FEATURE_DOWNLOAD_SDK").is_ok() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let lib_dir = Path::new(&manifest_dir).join("lib");
        let script = Path::new(&manifest_dir).join("scripts").join("download-sdk.sh");

        // Check if libs already exist
        let needs_download = if cfg!(target_os = "linux") {
            !lib_dir.join("libeego-SDK.so").exists()
        } else if cfg!(target_os = "windows") {
            !lib_dir.join("eego-SDK.dll").exists()
        } else {
            false // macOS: no vendor lib available
        };

        if needs_download {
            println!("cargo:warning=Downloading eego SDK vendor libraries...");
            let status = if cfg!(target_os = "windows") {
                Command::new("powershell")
                    .arg("-ExecutionPolicy").arg("Bypass")
                    .arg("-File")
                    .arg(Path::new(&manifest_dir).join("scripts").join("download-sdk.ps1"))
                    .status()
            } else {
                Command::new("bash")
                    .arg(&script)
                    .status()
            };

            match status {
                Ok(s) if s.success() => {
                    println!("cargo:warning=eego SDK downloaded successfully.");
                }
                Ok(s) => {
                    println!("cargo:warning=SDK download exited with: {}", s);
                }
                Err(e) => {
                    println!("cargo:warning=Failed to run download script: {}", e);
                }
            }
        }
    }
}
