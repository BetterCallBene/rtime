use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use interfaces::blackboard::BlackboardEntries;

#[derive(Debug, Serialize, Deserialize)]
pub struct LibraryConfig {
    pub name: String,
    pub path: Option<PathBuf>,
    pub attributes: Option<BlackboardEntries>,
}

pub type LibraryConfigs = Vec<LibraryConfig>;

#[derive(Debug, Serialize, Deserialize)]
pub struct RTConfig {
    pub libraries: LibraryConfigs,
}