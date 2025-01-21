use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
enum RTLibraryType {
    Service,
    Skill,
}

impl Default for RTLibraryType {
    fn default() -> Self {
        RTLibraryType::Skill
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RTCapabilityInfo {
    capability: String,
    entry: String,
}

impl RTCapabilityInfo {
    fn new(capability: &str, entry: &str) -> Self {
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
struct RTLibrarySummary {
    name: String,
    library_type: RTLibraryType,
    version: String,
    provides: Option<Vec<RTCapabilityInfo>>,
    requires: Option<Vec<String>>,
}

impl RTLibrarySummary {
    fn new(
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