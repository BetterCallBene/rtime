use clap::Parser;
use log::{info, trace, warn, debug};
use interfaces::blackboard::BlackboardEntry;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
mod helper;
mod rtlibrary;

#[derive(Parser, Debug)]
#[command(version = "0.1.0", about = "Kiss Runtime")]
struct Args {
    config: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct LibraryConfig {
    name: String,
    path: Option<PathBuf>,
    attributes: Option<Vec<BlackboardEntry>>
}

struct Components {
    
}

impl Components {
    fn new(_config: Vec<LibraryConfig>) -> Self {
        Self {}
    }
}

// struct RTLibrary {
//     lib: libloading::Library,
//     su
// }

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::init();

    let args = Args::parse();
    let config_path = args.config;

    info!(
        "Starting kiss runtime with config: {}",
        config_path.to_str().unwrap()
    );

    let config_str = std::fs::read_to_string(&config_path).map_err(|e| {
        format!(
            "Failed to read config file: {}. Reason: {}",
            config_path.to_str().unwrap(),
            e
        )
    })?;

    let config: Vec<LibraryConfig> = serde_yml::from_str(&config_str)
        .map_err(|e| format!("Failed to parse config: {}. Reason: {}", config_str, e))?;

    let components = Components::new(config);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    
}