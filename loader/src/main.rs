mod components;
mod config;
mod helper;
mod rtlibrary;
use clap::Parser;
use components::{create_caps, Components, ComponentsType};
use config::{LibraryConfigs, RTConfig};
use crossbeam_channel::{unbounded, Receiver, Sender};
use helper::{create_library_name, load_library, plugin_dir};
use interfaces::capabilities::Function;
use lazy_static::lazy_static;
use log::{debug, error, info, warn};
use rtlibrary::RTLibrary;
use std::{
    ffi::{c_char, c_int, c_void, CStr},
    path::PathBuf,
    sync::Arc,
};
use tokio::signal;
use tokio::time::{self, Duration as dur};

#[derive(Parser, Debug)]
#[command(version = "0.1.0", about = "Kiss Runtime")]
struct Args {
    config: PathBuf,
}

struct SenderReceiver {
    sender: Sender<String>,
    receiver: Receiver<String>,
}

lazy_static! {
    static ref TASK_SENDER_RECV: SenderReceiver = {
        let (sender, receiver) = unbounded::<String>();
        SenderReceiver {
            sender: sender,
            receiver: receiver,
        }
    };
}

fn load_libraries(config: &LibraryConfigs) -> Vec<RTLibrary> {
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

fn create_caps_blackboard(
    library_list: &Vec<ComponentsType>,
) -> interfaces::capabilities::Capabilities {
    let requires = vec!["blackboard".to_string()];
    create_caps(&requires, library_list)
}

fn subscribe_to_blackboard(
    caps: &interfaces::capabilities::Capabilities,
    key: &str,
    callback: extern "C" fn(*const c_char) -> c_int,
) -> Result<(), String> {
    let subscribe_cap = caps.get("blackboard_subscribe");

    if subscribe_cap.is_none() {
        return Err("Blackboard is not available".to_string());
    }
    let subscribe_fn: Function<
        extern "C" fn(*const c_char, *const c_char, callback: *mut c_void) -> c_int,
    > = unsafe { subscribe_cap.unwrap().get().unwrap() };

    let key = key.as_ptr() as *const c_char;
    let callback = callback as *mut c_void;

    let result = subscribe_fn(key, "loader\0".as_ptr() as *const c_char, callback);
    if result != 0 {
        return Err("Failed to subscribe to blackboard".to_string());
    }
    return Ok(());
}

fn get_string_from_blackboard(
    caps: &interfaces::capabilities::Capabilities,
    key: &str,
) -> Result<String, String> {
    let get_string_cap = caps.get("blackboard_get_string");

    if get_string_cap.is_none() {
        return Err("Blackboard is not available".to_string());
    }

    let get_string_fn: Function<unsafe extern "C" fn(ckey: *const c_char, cvalue: *mut c_char) -> c_int> =
        unsafe { get_string_cap.unwrap().get().unwrap() };

    let key = key.as_ptr() as *const c_char;
    let result = unsafe{get_string_fn(key, std::ptr::null_mut())};

    if result < 0 {
        return Err("Failed to get string from blackboard".to_string());
    }
    
    let mut buffer = vec![0u8; result as usize];

    let result = unsafe{get_string_fn(key, buffer.as_mut_ptr() as *mut c_char)};
    if result < 0 {
        return Err("Failed to get string from blackboard".to_string());
    }

    let result = unsafe {CStr::from_ptr(buffer.as_ptr() as *const c_char).to_str().map_err(|e| e.to_string())}?;
    Ok(result.to_string())
}

async fn runner(content: String) {
    
    
}

async fn task_manager(compoents: Arc<Components>) -> Result<(), String> {
    let mut interval = time::interval(dur::from_millis(100));

    let caps = create_caps_blackboard(&compoents.inner);
    let string_get_cap = caps.get("blackboard_get_string");

    if string_get_cap.is_none() {
        //panic!("Capability 'blackboard_set_string' not found");
        error!("Blackboard is not available");
        return Err("Blackboard is not available".to_string());
    }

    loop {
        let key = TASK_SENDER_RECV.receiver.try_recv();
        if key.is_ok() {
            let content = get_string_from_blackboard(&caps, "start_project\0").unwrap();
            tokio::spawn(runner(content));
        }
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

    extern "C" fn notify_callback(key: *const c_char) -> c_int {
        let key = unsafe { CStr::from_ptr(key).to_str().unwrap() };
        debug!("Callback called for key: {}", key);
        TASK_SENDER_RECV.sender.send(key.to_string()).unwrap();
        0
    }

    let caps = create_caps_blackboard(&components.inner);
    subscribe_to_blackboard(&caps, "start_project\0", notify_callback)?;

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
    use super::config::LibraryConfig;
    use super::*;
    use interfaces::blackboard::BlackboardEntries;
    use serial_test::serial;

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
        let config = vec![
            LibraryConfig::new("blackboard", None, None),
            LibraryConfig::new("blackboard", None, None),
        ];

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
        let config = vec![
            LibraryConfig::new("blackboard", None, None),
            LibraryConfig::new("webinterface", None, None),
        ];

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
