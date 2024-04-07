/* Credit where credit's due:
 * This build.rs is derived from: https://github.com/ysimonson/guile-sys.
 * Thanks to Yusuf Simonson for his work on the build script we use.
 * Makes things much, much easier... */

extern crate bindgen;

use cc::Build;

use std::env::var;
use std::path::PathBuf;
// use std::process::Command;
// use std::str;
//
// fn linker_args() -> (Vec<String>, Vec<String>) {
//     let mut search_args = Vec::new();
//     let mut lib_args = Vec::new();
//
//     for arg in config_args("link") {
//         if arg.starts_with("-L") {
//             search_args.push(arg[2..].to_string());
//         } else if arg.starts_with("-l") {
//             lib_args.push(arg[2..].to_string());
//         } else {
//             panic!("Unknown linker arg: {}", arg);
//         }
//     }
//
//     (search_args, lib_args)
// }

fn main() {
    // let (search_args, lib_args) = linker_args();

    // for arg in search_args {
    //     println!("cargo:rustc-link-search={}", arg);
    // }

    for arg in ["guile-3.0", "gc", "pthread", "dl"] {
        println!("cargo:rustc-link-lib={}", arg);
    }

    /* my addition: Mabe build.rs rebuild on change */
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.lock");

    let mut bindings = bindgen::Builder::default();



    // for arg in compiler_args {
    //     bindings = bindings.clang_arg(arg);
    // }
    bindings = bindings.clang_arg("-pthread");
    bindings = bindings.clang_arg("-I/usr/include/guile/3.0");

    let bindings = bindings
        .header("wrapper.h")
        .allowlist_function("scm_.*")
        .allowlist_var("scm_.*")
        .allowlist_var("SCM_.*")
        .generate()
        .expect("Unable to generate bindings.");

    let bindings_out_path =
        PathBuf::from(var("OUT_DIR").unwrap()).join("bindings.rs");

    bindings.write_to_file(bindings_out_path).unwrap();

    println!("cargo-rerun-if-changed=helper.c");

    Build::new().files(["helpers.c"]).compiler("gcc").include("/usr/include/guile/3.0").compile("helpers")
}
