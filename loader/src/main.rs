mod helper;
mod rtlibrary;
mod config;
mod components;
use clap::Parser;
use helper::{create_library_name, load_library, plugin_dir};
use config::{LibraryConfigs, RTConfig};
use rtlibrary::RTLibrary;
use components::{Components, create_caps};
use log::{info, warn, error};
use std::{path::PathBuf, sync::Arc};
use tokio::time::{self, Duration as dur};
use tokio::signal;

#[derive(Parser, Debug)]
#[command(version = "0.1.0", about = "Kiss Runtime")]
struct Args {
    config: PathBuf,
}

fn load_libraries(config: &LibraryConfigs) -> Vec<RTLibrary>{
    info!("Load libraries...");
    let mut libraries: Vec<RTLibrary> = Vec::new();

    for libconfig in config {
        let path = libconfig
            .path
            .clone()
            .or(Some(
                plugin_dir().join(create_library_name(&libconfig.name)),
            ))
            .unwrap();
        info!(
            "Try to loading library: {} ({})",
            libconfig.name,
            path.to_str().unwrap()
        );

        let library = load_library(&path).map_err(|e| {
            format!(
                "Failed loading library '{}' ({}): Reason: {}",
                libconfig.name,
                path.to_str().unwrap(),
                e
            )
        });

        match library {
            Ok(lib) => {
                info!("Successfull load library: {}", libconfig.name);
                match RTLibrary::new(lib, libconfig.attributes.clone()) {
                    Ok(rtlibrary) => {
                        let library_name = rtlibrary.summary.name.clone();

                        let found = libraries.iter().find(|lib| lib.name() == library_name);
                    
                        if found.is_some() {
                            warn!("Library '{}' already loaded. Skip loading.", library_name);
                            continue;
                        }
                        
                        libraries.push(rtlibrary);
                    }
                    Err(e) => {
                        warn!("Capability can not be load. Reason: {}", e)
                    }
                }
            }
            Err(e) => {
                warn!("{}", e);
            }
        }
    }
    libraries
}

async fn task_manager(compoents: Arc<Components>) {
    let mut interval = time::interval(dur::from_millis(100));

    let requires = vec!["blackboard".to_string()];
    let caps = create_caps(&requires, &compoents.inner);

    let string_get_cap = caps.get("blackboard_get_string");
    
    if string_get_cap.is_none() {
        //panic!("Capability 'blackboard_set_string' not found");
        error!("Blackboard is not available");
        return;
    }
    
    loop {

        interval.tick().await;
    }
}


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

    let config: RTConfig = serde_yml::from_str(&config_str)
        .map_err(|e| format!("Failed to parse config: {}. Reason: {}", config_str, e))?;


    let libraries = load_libraries(&config.libraries);
    let components = Components::new(libraries);
    components.start_services();

    let components = Arc::new(components);
    let task_handle = tokio::spawn(task_manager(components.clone()));

    // Wait for Ctrl+C signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Ctrl+C received! Shutting down...");
        }
        _ = task_handle => {
            info!("Main task finished");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use super::config::LibraryConfig;
    use interfaces::blackboard::BlackboardEntries;

    impl LibraryConfig {
        fn new(name: &str, path: Option<PathBuf>, attributes: Option<BlackboardEntries>) -> Self {
            LibraryConfig {
                name: name.to_string(),
                path: path,
                attributes: attributes,
            }
        }
    }
    
    #[serial]
    #[test_log::test]
    fn test_load_libraries() {
        let config = vec![LibraryConfig::new("blackboard", None, None)];

        let libraries = load_libraries(&config);
        assert_eq!(libraries.len(), 1);

        let found = libraries.iter().find(|lib| lib.name() == "blackboard");
        assert!(found.is_some());
    }

    #[serial]
    #[test_log::test]
    fn test_load_library_twice() {
        let config = vec![LibraryConfig::new("blackboard", None, None), LibraryConfig::new("blackboard", None, None)];

        let libraries = load_libraries(&config);
        assert_eq!(libraries.len(), 1);

        let found = libraries.iter().find(|lib| lib.name() == "blackboard");
        assert!(found.is_some());
    }

    #[serial]
    #[test_log::test]
    fn test_create_component() {
        let config = vec![LibraryConfig::new("blackboard", None, None)];

        let libraries = load_libraries(&config);
        assert_eq!(libraries.len(), 1);

        let found = libraries.iter().find(|lib| lib.name() == "blackboard");
        assert!(found.is_some());

        let components = Components::new(libraries);
        assert_eq!(components.inner.len(), 1);
    }
    
    // #[serial]
    // #[test_log::test]
    // fn test_

    #[serial]
    #[test_log::test]
    fn test_create_caps() {
        
        let config = vec![LibraryConfig::new("blackboard", None, None), LibraryConfig::new("webinterface", None, None)];

        let libraries = load_libraries(&config);
        assert_eq!(libraries.len(), 2);

        let found = libraries.iter().find(|lib| lib.name() == "blackboard");
        assert!(found.is_some());

        let components = Components::new(libraries);
        assert_eq!(components.inner.len(), 2);

        let requires = vec!["blackboard".to_string()];
        let caps = create_caps(&requires, &components.inner);

        assert_eq!(caps.len(), 16);

        let string_set_cap = caps.get("blackboard_set_string");
        assert!(string_set_cap.is_some());
        // let string_set_cap = string_set_cap.unwrap();

        
        // let result = unsafe {
        //     let f: Function<unsafe extern "C" fn(*const c_char, *const c_char) -> c_int> = string_set_cap.get().unwrap();
        //     f("example_key\0".as_ptr() as *const c_char, "test\0".as_ptr() as *const c_char)
        // };

        // assert_eq!(result, 0);
    }
}
