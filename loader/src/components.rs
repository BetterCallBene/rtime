use super::rtlibrary;
use libloading::Symbol;
use log::{error, info, trace, warn};
use rtlibrary::{RTLibrary, RTLibraryType};
use std::ffi::{c_char, c_int, c_void};

pub trait Component {
    fn run(
        &self,
        function: &str,
        caps: &interfaces::capabilities::Capabilities,
    ) -> Result<i32, String> {
        let library = &self.library().library;
        let attr = self.attributes();
        let result = unsafe {
            library.get(function.as_bytes()).map(
                |f: Symbol<
                    unsafe extern "C" fn(
                        &interfaces::bindings::Capabilities,
                        *const c_char,
                    ) -> c_int,
                >| { f(caps.inner(), attr.as_ptr() as *const c_char) },
            )
        };
        match result {
            Ok(r) => Ok(r),
            Err(e) => Err(format!(
                "Function '{}' can not be called. Reason: {}",
                function, e
            )),
        }
    }
    fn attributes(&self) -> &str;
    fn library(&self) -> &RTLibrary;
    fn requires(&self) -> &Vec<String>;
}

pub enum ComponentsType {
    Service(Service),
    Skill(Skill),
}

pub type ComponentsVec = Vec<ComponentsType>;

pub struct Skill {
    pub library: RTLibrary,
    pub requires: Vec<String>,
}

pub struct Service {
    pub library: RTLibrary,
    pub requires: Vec<String>,
}

impl Component for Skill {
    fn library(&self) -> &RTLibrary {
        &self.library
    }

    fn requires(&self) -> &Vec<String> {
        &self.requires
    }

    fn attributes(&self) -> &str {
        if self.library.config_attr_str.is_none() {
            ""
        } else {
            self.library.config_attr_str.as_ref().unwrap().as_str()
        }
    }
}

impl Component for Service {
    fn library(&self) -> &RTLibrary {
        &self.library
    }

    fn requires(&self) -> &Vec<String> {
        &self.requires
    }

    fn attributes(&self) -> &str {
        if self.library.config_attr_str.is_none() {
            ""
        } else {
            self.library.config_attr_str.as_ref().unwrap().as_str()
        }
    }
}

impl Components {
    pub fn new(mut libraries: Vec<RTLibrary>) -> Self {
        let mut inner: ComponentsVec = Vec::new();
        while let Some(lib) = libraries.pop() {
            let library_type = lib.summary.library_type.clone();

            let component: ComponentsType = match library_type {
                RTLibraryType::Service => ComponentsType::Service(Service::new(lib).unwrap()),
                RTLibraryType::Skill => ComponentsType::Skill(Skill::new(lib).unwrap()),
            };

            inner.push(component);
        }
        Self { inner }
    }

    pub fn start_services(&self) {
        for component in self.inner.iter().rev() {
            if let ComponentsType::Service(service) = component {
                service
                    .start(&create_caps(&service.requires(), &self.inner))
                    .map_err(|e| {
                        warn!(
                            "Service '{}' can not be started. Reason: {}",
                            service.library.summary.name, e
                        );
                    })
                    .unwrap();
            }
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
            library: library,
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
            library: library,
        })
    }

    fn start(&self, caps: &interfaces::capabilities::Capabilities) -> Result<i32, String> {
        Component::run(self, "start", caps)
    }

    fn stop(&self) {
        unsafe {
            let library = &self.library.library;
            let result = library
                .get("stop".as_bytes())
                .map(|f: Symbol<unsafe extern "C" fn() -> c_int>| f());
            match result {
                Ok(_) => {
                    info!("Service '{}' stopped", self.library.summary.name);
                }
                Err(e) => {
                    warn!(
                        "Service '{}' can not be stopped. Reason: {}",
                        self.library.summary.name, e
                    );
                }
            }
        }
    }
}

pub struct Components {
    pub inner: ComponentsVec,
}

fn get_capability_fn<'a>(
    library: &'a RTLibrary,
    capability_entry: &str,
) -> Result<Symbol<'a, unsafe extern "C" fn() -> *mut c_void>, String> {
    unsafe {
        library
            .library
            .get(capability_entry.as_bytes())
            .map(|f: Symbol<unsafe extern "C" fn() -> *mut c_void>| f)
            .map_err(|e| format!("Capability cannot be loaded. Reason: {}", e))
    }
}

pub fn create_caps(
    requires: &Vec<String>,
    libraries: &ComponentsVec,
) -> interfaces::capabilities::Capabilities {
    let mut caps = interfaces::capabilities::Capabilities::new();

    for require_lib in requires {
        let lib = libraries.iter().find(|lib| match lib {
            ComponentsType::Service(service) => service.library.summary.name == *require_lib,
            ComponentsType::Skill(skill) => skill.library.summary.name == *require_lib,
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

            let capability_fn = match lib {
                Some(ComponentsType::Service(service)) => {
                    get_capability_fn(&service.library, capability_entry.as_str())
                }
                Some(ComponentsType::Skill(skill)) => {
                    get_capability_fn(&skill.library, capability_entry.as_str())
                }
                None => {
                    let error_string = format!(
                        "Capability '{}' not found in '{}'",
                        capability_name, require_lib
                    );
                    error!("{}", error_string);
                    Err(error_string)
                }
            };

            if capability_fn.is_err() {
                panic!(
                    "System configuration error. Reason: {}",
                    capability_fn.unwrap_err()
                );
            }

            let capability_fn = capability_fn.unwrap();
            caps.add(interfaces::capabilities::Capability::new(
                &capability_name,
                unsafe { capability_fn.try_as_raw_ptr().unwrap() },
            ));
        }
    }
    caps
}
