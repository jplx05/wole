// Build script to set stack size on Windows
// This ensures tests have enough stack space for directory traversal
// Updated to force rebuild

fn main() {
    // Get the target OS and environment
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    
    eprintln!("[BUILD.RS] target_os={} target_env={}", target_os, target_env);
    
    if target_os == "windows" {
        if target_env == "msvc" {
            // MSVC linker: set stack size to 16MB (16777216 bytes)
            eprintln!("[BUILD.RS] Setting MSVC stack to 16MB");
            println!("cargo:rustc-link-arg=/STACK:16777216");
        } else if target_env == "gnu" {
            // GNU/MinGW linker: set stack size to 16MB
            eprintln!("[BUILD.RS] Setting GNU stack to 16MB");
            println!("cargo:rustc-link-arg=-Wl,--stack,16777216");
        } else {
            eprintln!("[BUILD.RS] WARNING: Unknown target_env={}", target_env);
        }
    } else {
        eprintln!("[BUILD.RS] Not Windows, skipping stack config");
    }
    
    println!("cargo:rerun-if-changed=build.rs");
}
