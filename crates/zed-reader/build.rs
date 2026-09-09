use std::env;
use std::path::PathBuf;

fn main() {
    let zed_dir = env::var("ZED_SDK_DIR").unwrap_or_else(|_| "/usr/local/zed".to_string());
    let cuda_dir = env::var("CUDA_DIR").unwrap_or_else(|_| "/usr/local/cuda".to_string());

    let zed_include = PathBuf::from(&zed_dir).join("include");
    let zed_lib = PathBuf::from(&zed_dir).join("lib");
    let cuda_include = PathBuf::from(&cuda_dir).join("include");
    let cuda_lib = PathBuf::from(&cuda_dir).join("lib64");

    if !zed_include.is_dir() {
        panic!(
            "ZED SDK not found at {} (set ZED_SDK_DIR to override)",
            zed_dir
        );
    }

    cc::Build::new()
        .cpp(true)
        .file("cpp/zed_camera.cpp")
        .include(&zed_include)
        .include(&cuda_include)
        .flag_if_supported("-std=c++17")
        .compile("zed_camera");

    println!("cargo:rustc-link-search=native={}", zed_lib.display());
    println!("cargo:rustc-link-search=native={}", cuda_lib.display());
    println!("cargo:rustc-link-lib=dylib=sl_zed");
    println!("cargo:rustc-link-lib=dylib=cudart");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", zed_lib.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", cuda_lib.display());

    println!("cargo:rerun-if-changed=cpp/zed_camera.cpp");
    println!("cargo:rerun-if-changed=cpp/zed_camera.h");
    println!("cargo:rerun-if-env-changed=ZED_SDK_DIR");
    println!("cargo:rerun-if-env-changed=CUDA_DIR");
}
