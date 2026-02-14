use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let src_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("src");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    println!("cargo::rerun-if-changed=src/audio_c.h");

    match target_os.as_str() {
        "macos" => {
            build_macos(&src_dir);
            generate_bindings(&src_dir, &out_dir);
        }
        "linux" => {
            let has_pulse = env::var("CARGO_FEATURE_PULSE").is_ok();
            let has_pipewire = env::var("CARGO_FEATURE_PIPEWIRE").is_ok();

            if has_pulse && has_pipewire {
                panic!("features \"pulse\" and \"pipewire\" are mutually exclusive");
            }

            if has_pipewire {
                build_linux_pipewire(&src_dir);
            } else {
                build_linux_pulse(&src_dir);
            }

            generate_bindings(&src_dir, &out_dir);
        }
        "windows" => {
            // windows-rs を使用するため C++ コンパイル不要
            // bindgen も不要
        }
        _ => {
            panic!("Unsupported target OS: {}", target_os);
        }
    }
}

fn build_linux_pulse(src_dir: &Path) {
    println!("cargo::rerun-if-changed=src/audio_pulse.c");

    let pulse = pkg_config::Config::new()
        .probe("libpulse")
        .expect("libpulse not found. Please install libpulse-dev");

    let mut build = cc::Build::new();
    build.file(src_dir.join("audio_pulse.c"));

    for path in &pulse.include_paths {
        build.include(path);
    }

    build.compile("audio_pulse");
}

fn build_linux_pipewire(src_dir: &Path) {
    println!("cargo::rerun-if-changed=src/audio_pipewire.c");

    let pipewire = pkg_config::Config::new()
        .probe("libpipewire-0.3")
        .expect("libpipewire-0.3 not found. Please install libpipewire-0.3-dev");

    let mut build = cc::Build::new();
    build.file(src_dir.join("audio_pipewire.c"));

    for path in &pipewire.include_paths {
        build.include(path);
    }

    build.compile("audio_pipewire");
}

fn build_macos(src_dir: &Path) {
    println!("cargo::rerun-if-changed=src/audio_c.m");

    // Objective-C ファイルをコンパイル
    cc::Build::new()
        .file(src_dir.join("audio_c.m"))
        .flag("-fobjc-arc")
        .compile("audio_c");

    // macOS フレームワークをリンク
    println!("cargo::rustc-link-lib=framework=AudioToolbox");
    println!("cargo::rustc-link-lib=framework=CoreAudio");
    println!("cargo::rustc-link-lib=framework=Foundation");
}

fn generate_bindings(src_dir: &Path, out_dir: &Path) {
    let bindings = bindgen::Builder::default()
        .header(src_dir.join("audio_c.h").to_str().unwrap())
        .allowlist_function("audio_.*")
        .allowlist_type("AudioDevice")
        .allowlist_type("AudioSession")
        .allowlist_type("AudioFrameCallback")
        .allowlist_var("AUDIO_FORMAT_.*")
        .allowlist_var("AUDIO_DEVICE_TYPE_.*")
        .derive_default(true)
        .derive_debug(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Failed to generate bindings");

    let bindings_path = out_dir.join("bindings.rs");
    bindings
        .write_to_file(&bindings_path)
        .expect("Failed to write bindings");
}
