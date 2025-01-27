use serde::{Deserialize, Serialize};
use libloading::{Library, Symbol};
use std::ffi::CStr;

use super::helper::{create_library_name, plugin_dir, load_library};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum RTLibraryType {
    Service,
    Skill,
}

impl Default for RTLibraryType {
    fn default() -> Self {
        RTLibraryType::Skill
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RTCapabilityInfo {
    pub capability: String,
    pub entry: String,
}

impl RTCapabilityInfo {
    pub fn new(capability: &str, entry: &str) -> Self {
        Self {
            capability: capability.to_string(),
            entry: entry.to_string(),
        }
    }
}

impl Clone for RTCapabilityInfo {
    fn clone(&self) -> Self {
        return RTCapabilityInfo::new(&self.capability, &self.entry);
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RTLibrarySummary {
    pub name: String,
    pub library_type: RTLibraryType,
    pub version: String,
    pub provides: Option<Vec<RTCapabilityInfo>>,
    pub requires: Option<Vec<String>>,
}

impl RTLibrarySummary {
    pub fn new(
        name: &str,
        library_type: &RTLibraryType,
        version: &str,
        provides: &Option<Vec<RTCapabilityInfo>>,
        requires: &Option<Vec<String>>,
    ) -> Self {
        RTLibrarySummary {
            name: name.to_string(),
            library_type: library_type.clone(),
            version: version.to_string(),
            provides: provides.clone(),
            requires: requires.clone(),
        }
    }
}

impl Clone for RTLibrarySummary {
    fn clone(&self) -> Self {
        return RTLibrarySummary::new(
            &self.name,
            &self.library_type,
            &self.version,
            &self.provides,
            &self.requires,
        );
    }
}

#[derive(Debug)]
pub struct RTLibrary {
    pub library: Library,
    pub summary: RTLibrarySummary,
}

impl RTLibrary {
    pub fn new(library: Library) -> Result<Self, String> {
        unsafe {
            let symbol: Symbol<unsafe extern "C" fn() -> *const ::std::os::raw::c_char> = library
                .get(b"summary")
                .map_err(|_e| "summary symbol not found".to_string())?;
            let cstr_i8 = symbol();

            if (cstr_i8 as *const u8).is_null() {
                return Err("Summary can not parsed.\nReason: data is null".to_string());
            }

            let summary_yaml_str = CStr::from_ptr(cstr_i8)
                .to_str()
                .map_err(|e| format!("Failed to get summary: Reason: {}", e))?;
            let summary: RTLibrarySummary = serde_yml::from_str(&summary_yaml_str).map_err(|e| {
                format!(
                    "Failed to parse summary: {}. Reason: {}",
                    summary_yaml_str, e
                )
            })?;

            Ok(Self {
                library,
                summary: summary,
            })
        }
    }

    pub fn name(&self) -> &str {
        &self.summary.name
    }

    pub fn library_type(&self) -> &RTLibraryType {
        &self.summary.library_type
    }

    pub fn version(&self) -> &str {
        &self.summary.version
    }

    pub fn is_service(&self) -> bool {
        self.summary.library_type == RTLibraryType::Service
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::fixture;
    use rstest::rstest;
    use serial_test::serial;
    
    use std::path::PathBuf;
    use log::error;

    #[fixture]
    fn plugin_dir_fixture() -> PathBuf {
        plugin_dir()
    }

    #[fixture]
    fn blackboard_plugin_path(plugin_dir_fixture: PathBuf) -> PathBuf {
        plugin_dir_fixture.join(create_library_name("blackboard"))
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_load_library(blackboard_plugin_path: PathBuf) {
        let result = load_library(&blackboard_plugin_path);
        assert!(result.is_ok());
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_load_library_fail() {
        let result = load_library(&PathBuf::from("non_existent_path"));
        assert!(result.is_err());
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_load_rtlibrary(blackboard_plugin_path: PathBuf) {
        let library = load_library(&blackboard_plugin_path);
        assert!(library.is_ok());
        let rtlibrary = RTLibrary::new(library.unwrap());
        match &rtlibrary {
            Ok(_) => assert!(true),
            Err(e) => {
                error!("Failed to load skill: {}", e);
                assert!(false);
            }
        }

        let skill = rtlibrary.unwrap();
        assert_eq!(skill.summary.name, "blackboard");
    }

    // #[rstest]
    // #[test_log::test]
    // fn test_create_capabilties(blackboard_plugin_path: PathBuf) {
    //     let library = load_library(&blackboard_plugin_path);
    //     assert!(library.is_ok());
    //     let library = RTLibrary::new(library.unwrap());
    //     assert!(library.is_ok());
    //     let skill = library.unwrap();

    //     assert_eq!(skill.summary.provides.is_some(), true);

    //     let provides = skill.summary.provides.unwrap();
    //     let mut caps = Capabilities::new();

    //     for capability in provides {
    //         let capability_name = capability.capability;
    //         let capability_entry = capability.entry.as_bytes();

    //         unsafe {
    //             let capability_fn: Symbol<unsafe extern "C" fn() -> *mut c_void> =
    //                 skill.library.get(capability_entry).unwrap();
    //             let capability_fn = capability_fn.try_as_raw_ptr().unwrap();
    //             caps.add(Capability::new(&capability_name, capability_fn))
    //         };
    //     }

    //     assert_eq!(caps.len(), 16);

    //     let cap1 = caps.get("blackboard_set_string");
    //     assert!(cap1.is_some());

    //     let cap1 = cap1.unwrap();
    //     let result = unsafe {
    //         let set_string_fn: Function<
    //             unsafe extern "C" fn(key: *const c_char, value: *const c_char) -> c_int,
    //         > = cap1.get().unwrap();
    //         let key = "example\0".as_bytes().as_ptr() as *const c_char;
    //         let value = "Hello, World!\0".as_bytes().as_ptr() as *const c_char;
    //         set_string_fn(key, value)
    //     };

    //     assert_eq!(result, 0);

    //     let cap2 = caps.get("blackboard_get_string");
    //     assert!(cap2.is_some());

    //     let cap2 = cap2.unwrap();
    //     let mut buffer = vec![0u8; 14];
    //     let result = unsafe {
    //         let get_string_fn: Function<
    //             unsafe extern "C" fn(key: *const c_char, value: *mut c_char) -> c_int,
    //         > = cap2.get().unwrap();
    //         let key = "example\0".as_bytes().as_ptr() as *const c_char;
    //         get_string_fn(key, buffer.as_mut_ptr() as *mut c_char)
    //     };

    //     assert_eq!(result, 14);
    //     let result = unsafe {
    //         CStr::from_ptr(buffer.as_ptr() as *const c_char)
    //             .to_str()
    //             .unwrap()
    //     };
    //     assert_eq!(result, "Hello, World!");
    // }
}