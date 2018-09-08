extern crate cc;

use std::env;

fn main() {
    let mut builder = cc::Build::new()
        .cpp(true)
        .extra_warnings(true)
        .warnings_into_errors(true)
        .static_flag(true)
        .flag("-std=c++11")
        .flag_if_supported("-Wno-implicit-fallthrough")
        // .flag("-Wno-unused-function")
        // .flag_if_supported("-fomit-frame-pointer")
        // .flag_if_supported("-fbuiltin")
        // .flag_if_supported("-funroll-loops") // GCC only
        .include("longhair/")
        .file("longhair/cauchy_256.cpp")
        .file("longhair/gf256.cpp")
        .clone();

    if env::var("TARGET").unwrap().contains("arm") {
        builder.define("LINUX_ARM", None);
    }

    builder.compile("longhair");
}
