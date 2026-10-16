//! Build script: link flags for the optional Nabto client SDK.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=NABTO_SDK_DIR");
    if std::env::var_os("CARGO_FEATURE_NABTO_SDK").is_some() {
        link_nabto_sdk();
    }
}

/// The closed-source Nabto 4 client SDK is not part of this repository. Put it under
/// `vendor/nabto4/<linux64|mac64>/lib` at the repository root, or point `NABTO_SDK_DIR` at a
/// directory with that layout.
fn link_nabto_sdk() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if arch != "x86_64" {
        panic!("the Nabto 4 client SDK is only available for x86_64 (target arch is {arch})");
    }
    let dir = match os.as_str() {
        "linux" => "linux64",
        "macos" => "mac64",
        other => panic!("unsupported target OS {other}"),
    };
    let root = std::env::var("NABTO_SDK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../vendor/nabto4")
        });
    let lib = root.join(dir).join("lib");
    if !lib.join("libnabto_client_api_static.a").exists() {
        panic!("Nabto SDK not found: expected libnabto_client_api_static.a in {} (see README, \"Nabto SDK\")", lib.display());
    }
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=static=nabto_client_api_static");
    println!("cargo:rustc-link-lib=static=nabto_static_external");
    if os == "linux" {
        // the SDK was built with GCC 4.9 and registers constructors in .ctors sections, which
        // rustc's default linker (lld) does not run: link with GNU ld instead
        println!("cargo:rustc-link-arg=-fuse-ld=bfd");
        for lib in ["stdc++", "pthread", "dl", "rt", "m"] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    } else {
        println!("cargo:rustc-link-lib=dylib=c++");
        for fw in ["CoreFoundation", "Security", "SystemConfiguration", "IOKit"] {
            println!("cargo:rustc-link-lib=framework={fw}");
        }
    }
}
