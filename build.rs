// Build script to set stack size on Windows
// This ensures tests have enough stack space for directory traversal

fn main() {
    // Get the target OS and environment
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    
    if target_os == "windows" {
        if target_env == "msvc" {
            // MSVC linker: set stack size to 16MB (16777216 bytes)
            // Increased from 8MB to handle very deep directory structures
            // Apply to all binaries including tests
            println!("cargo:rustc-link-arg=/STACK:16777216");
            println!("cargo:rustc-link-arg-bins=/STACK:16777216");
            println!("cargo:rustc-link-arg-tests=/STACK:16777216");
        } else if target_env == "gnu" {
            // GNU/MinGW linker: set stack size to 16MB
            println!("cargo:rustc-link-arg=-Wl,--stack,16777216");
            println!("cargo:rustc-link-arg-bins=-Wl,--stack,16777216");
            println!("cargo:rustc-link-arg-tests=-Wl,--stack,16777216");
        }
    }
}
