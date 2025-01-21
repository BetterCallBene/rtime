use libloading::{Library, Symbol};
use std::{env, env::consts::OS, path::PathBuf, };

fn plugin_dir() -> PathBuf {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let cw = env::current_dir().unwrap();
    cw.ancestors().nth(1).unwrap().join("plugins").join(profile)
}

fn create_library_name(pkg_name: &str) -> String {
    let lib_prefix = if OS == "windows" { "" } else { "lib" };

    let ext = match OS {
        "windows" => "dll",
        "macos" => "dylib",
        _ => "so",
    };

    format!("{}{}.{}", lib_prefix, pkg_name, ext)
}

fn load_library(path: &PathBuf) -> Result<Library, String> {
    unsafe { Library::new(path).map_err(|e| e.to_string()) }
}
