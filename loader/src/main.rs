mod helper;
mod rtlibrary;
use clap::Parser;
use helper::{create_library_name, load_library, plugin_dir};
use interfaces::blackboard::{self, BlackboardEntries};
use log::{debug, info, trace, warn, error};
use rtlibrary::{RTLibrary, RTLibraryType};
use serde::{Deserialize, Serialize};
use libloading::Symbol;
use serde_yml::mapping::Iter;
use std::{any::Any, collections::HashMap, os::raw::{c_char, c_int, c_void}, path::PathBuf, sync::Arc};
use tokio::time::{self, Duration as dur};
use tokio::signal;

#[derive(Parser, Debug)]
#[command(version = "0.1.0", about = "Kiss Runtime")]
struct Args {
    config: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct LibraryConfig {
    name: String,
    path: Option<PathBuf>,
    attributes: Option<BlackboardEntries>,
}

impl LibraryConfig {
    fn new(name: &str, path: Option<PathBuf>, attributes: Option<BlackboardEntries>) -> Self {
        Self {
            name: name.to_string(),
            path,
            attributes,
        }
    }
}

type LibraryConfigs = Vec<LibraryConfig>;

#[derive(Debug, Serialize, Deserialize)]
struct RTConfig {
    libraries: LibraryConfigs,
}

trait Component: {

    fn run(&self, function: &str, caps: &interfaces::capabilities::Capabilities) -> Result<i32, String>
    {
        let library = &self.library().library;
        let attr = self.attributes();
        let result = unsafe {
            library.get(function.as_bytes()).map(|f: Symbol<unsafe extern "C" fn(&interfaces::bindings::Capabilities, *const c_char) -> c_int>| {
                f(caps.inner(), attr.as_ptr() as *const c_char)
            })
        };
        match result {
            Ok(r) => Ok(r),
            Err(e) => Err(format!("Function '{}' can not be called. Reason: {}", function, e))
        }
    }
    fn attributes(&self)-> &str;
    fn library(&self) -> &RTLibrary;
    fn requires(&self) -> &Vec<String>;
}

enum ComponentsType{
    Service(Service),
    Skill(Skill)
}

type ComponentsVec = Vec<ComponentsType>;

struct Skill{
    library: RTLibrary,
    requires: Vec<String>
}

struct Service{
    library: RTLibrary,
    requires: Vec<String>
}

impl Component for Skill  {
    fn library(&self) -> &RTLibrary {
        &self.library
    }

    fn requires(&self) -> &Vec<String> {
        &self.requires
    }

    fn attributes(&self)-> &str {
        if self.library.config_attr_str.is_none() {
            ""
        } else {
            self.library.config_attr_str.as_ref().unwrap().as_str()
        }
    }
}

impl Component for Service  {
    fn library(&self) -> &RTLibrary {
        &self.library
    }

    fn requires(&self) -> &Vec<String> {
        &self.requires
    }

    fn attributes(&self)-> &str {
        if self.library.config_attr_str.is_none() {
            ""
        } else {
            self.library.config_attr_str.as_ref().unwrap().as_str()
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Skill {
    fn new(library: RTLibrary) -> Result<Self, String> {
        Ok(Self {
            requires: if library.summary.requires.is_some() {
                library.summary.requires.clone().unwrap()
            } else {
                Vec::new()
            },
            library: library
        })
    }

    fn run(&self, caps: &interfaces::capabilities::Capabilities) -> Result<i32, String> {
        Component::run(self, "run", caps)
    }
}


impl Service {
    fn new(library: RTLibrary) -> Result<Self, String> {
        Ok(Self {
            requires: if library.summary.requires.is_some() {
                library.summary.requires.clone().unwrap()
            } else {
                Vec::new()
            },
            library: library
        })
    }

    fn start(&self, caps: &interfaces::capabilities::Capabilities) -> Result<i32, String> {
        Component::run(self, "start", caps)
    }

    fn stop(&self) {
        unsafe {
            let library = &self.library.library;
            let result = library.get("stop".as_bytes()).map(|f: Symbol<unsafe extern "C" fn() -> c_int>| {
                f()
            });
            match result {
                Ok(_) => {
                    info!("Service '{}' stopped", self.library.summary.name);
                }
                Err(e) => {
                    warn!("Service '{}' can not be stopped. Reason: {}", self.library.summary.name, e);
                }
            }
        }
    }
}

struct Components {
    inner: ComponentsVec,
}

fn get_capability_fn<'a>(library: &'a RTLibrary, capability_entry: &str) -> Result<Symbol<'a, unsafe extern "C" fn() -> *mut c_void>, String> {
    unsafe {library.library.get(capability_entry.as_bytes())
        .map(|f: Symbol<unsafe extern "C" fn() -> *mut c_void>| f)
        .map_err(|e| format!("Capability cannot be loaded. Reason: {}", e))}
}

fn create_caps(
    requires: &Vec<String>,
    libraries: &ComponentsVec,
) -> interfaces::capabilities::Capabilities {
    let mut caps = interfaces::capabilities::Capabilities::new();

    for require_lib in requires {
        let lib = libraries.iter().find(|lib| {
            match lib {
                ComponentsType::Service(service) => service.library.summary.name == *require_lib,
                ComponentsType::Skill(skill) => skill.library.summary.name == *require_lib
            }
        });
        
        let provides = match lib {
            Some(ComponentsType::Service(service)) => &service.library.summary.provides,
            Some(ComponentsType::Skill(skill)) => &skill.library.summary.provides,
            None => {
                warn!("Library '{}' not found", require_lib);
                continue;
            }
        };
        

        let provides = provides.as_ref().unwrap();

        for capability in provides {
            let capability_name = capability.capability.clone();
            let capability_entry = capability.entry.clone();

            trace!("Capability: {}", capability_name);
            trace!("Entry: {}", capability_entry);

            let capability_fn = unsafe {

                 match lib{
                    Some(ComponentsType::Service(service)) => {
                        get_capability_fn(&service.library, capability_entry.as_str())
                    }
                    Some(ComponentsType::Skill(skill)) => {
                        get_capability_fn(&skill.library, capability_entry.as_str())
                    }
                    None => {
                        let error_string = format!("Capability '{}' not found in '{}'", capability_name, require_lib);
                        error!("{}", error_string);
                        Err(error_string)
                    }
                }
            };

            if capability_fn.is_err() {
                panic!("System configuration error. Reason: {}", capability_fn.unwrap_err());
            }

            let capability_fn = capability_fn.unwrap();   
            caps.add(interfaces::capabilities::Capability::new(
                &capability_name,
                unsafe{capability_fn.try_as_raw_ptr().unwrap()}
            ));
        }
    }
    caps
}

impl Components {
    fn new(mut libraries: Vec<RTLibrary>) -> Self {
        let mut inner:ComponentsVec = Vec::new();
        while let Some(lib) = libraries.pop() {
            let library_type = lib.summary.library_type.clone();
    
            let component:ComponentsType = match library_type {
                RTLibraryType::Service => {
                    ComponentsType::Service(Service::new(lib).unwrap())
                }
                RTLibraryType::Skill => {
                    ComponentsType::Skill(Skill::new(lib).unwrap())
                }
            };
    
            inner.push(component);
        }
        Self {
            inner
        }
    }

    fn start_services(&self)
    {
        for component in self.inner.iter().rev(){
            
            if let ComponentsType::Service(service) = component {
                
                service.start(&create_caps(&service.requires, &self.inner)).map_err(|e| {
                    warn!("Service '{}' can not be started. Reason: {}", service.library.summary.name, e);
                }).unwrap();
                
            }
            
        }
    }
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
    
    let task_handle = tokio::spawn(async move {
        let mut interval = time::interval(dur::from_millis(100));

        let requires = vec!["blackboard".to_string()];
        let caps = create_caps(&requires, &components.inner);

        let string_get_cap = caps.get("blackboard_get_string");
        
        if string_get_cap.is_none() {
            //panic!("Capability 'blackboard_set_string' not found");
            error!("Blackboard is not available");
            return;
        }

        
        loop {

            interval.tick().await;
        }
    });

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
    use interfaces::capabilities::Function;
    use serial_test::serial;
    
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
