// bindgen path_to_header.h -o bindings.rs
use std::path::PathBuf;


fn main(){
    println!("cargo:rerun-if-changed=caps.h");
    
    let bindings = bindgen::Builder::default()
        .header("caps.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from("src");
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

}