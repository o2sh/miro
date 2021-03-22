extern crate gl_generator;

use gl_generator::{Api, Fallbacks, Profile, Registry};
use std::env;
use std::fs::File;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/window/build.rs");
    let dest = PathBuf::from(&env::var("OUT_DIR").unwrap());
    let target = env::var("TARGET").unwrap();
    if !target.contains("macos") {
        let mut file = File::create(&dest.join("egl_bindings.rs")).unwrap();
        let reg = Registry::new(Api::Egl, (1, 5), Profile::Core, Fallbacks::All, []);
        reg.write_bindings(gl_generator::StructGenerator, &mut file).unwrap()
    }
}
