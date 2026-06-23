use std::env;
use std::path::{Path, PathBuf};

use bindgen::Builder;

fn main() {
    // 各 feature に対応する cfg を宣言する
    println!("cargo::rustc-check-cfg=cfg(enable_coreaudio)");
    println!("cargo::rustc-check-cfg=cfg(enable_pulse)");
    println!("cargo::rustc-check-cfg=cfg(enable_pipewire)");
    println!("cargo::rustc-check-cfg=cfg(enable_wasapi)");
    println!("cargo::rustc-check-cfg=cfg(enable_default_coreaudio)");
    println!("cargo::rustc-check-cfg=cfg(enable_default_pulse)");
    println!("cargo::rustc-check-cfg=cfg(enable_default_pipewire)");
    println!("cargo::rustc-check-cfg=cfg(enable_default_wasapi)");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    match target_os.as_str() {
        "macos" => {
            let has_coreaudio = env::var("CARGO_FEATURE_COREAUDIO").is_ok();
            if has_coreaudio {
                println!("cargo::rustc-cfg=enable_coreaudio");
                println!("cargo::rustc-cfg=enable_default_coreaudio");
            }
        }
        "linux" => {
            let has_pulse = env::var("CARGO_FEATURE_PULSE").is_ok();
            let has_pipewire = env::var("CARGO_FEATURE_PIPEWIRE").is_ok();

            if has_pulse {
                println!("cargo::rustc-cfg=enable_pulse");
                // pulse が有効なら常に pulse がデフォルト
                println!("cargo::rustc-cfg=enable_default_pulse");
            }
            if has_pipewire {
                println!("cargo::rustc-cfg=enable_pipewire");
                if !has_pulse {
                    // pulse が無効な場合のみ pipewire がデフォルト
                    println!("cargo::rustc-cfg=enable_default_pipewire");
                }
            }
        }
        "windows" => {
            if env::var("CARGO_FEATURE_WASAPI").is_ok() {
                println!("cargo::rustc-cfg=enable_wasapi");
                println!("cargo::rustc-cfg=enable_default_wasapi");
            }
        }
        _ => panic!("Unsupported target OS: {}", target_os),
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let src_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("src");

    match target_os.as_str() {
        "macos" => {
            if env::var("CARGO_FEATURE_COREAUDIO").is_ok() {
                build_coreaudio(&src_dir);
                let builder =
                    Builder::default().header(src_dir.join("audio_coreaudio.h").to_str().unwrap());
                generate_bindings(builder, "bindings_coreaudio.rs", &out_dir);
            }
        }
        "linux" => {
            let has_pulse = env::var("CARGO_FEATURE_PULSE").is_ok();
            let has_pipewire = env::var("CARGO_FEATURE_PIPEWIRE").is_ok();

            if has_pulse {
                build_linux_pulse(&src_dir);
            }
            if has_pipewire {
                build_linux_pipewire(&src_dir);
            }
            if has_pulse || has_pipewire {
                let mut builder = Builder::default();
                if has_pulse {
                    builder = builder.header(src_dir.join("audio_pulse.h").to_str().unwrap());
                }
                if has_pipewire {
                    builder = builder.header(src_dir.join("audio_pipewire.h").to_str().unwrap());
                }
                generate_bindings(builder, "bindings_linux.rs", &out_dir);
            }
        }
        "windows" => {
            // windows-rs を使用するため C コンパイル不要、bindgen も不要
        }
        _ => panic!("Unsupported target OS: {}", target_os),
    }
}

fn build_coreaudio(src_dir: &Path) {
    println!("cargo::rerun-if-changed=src/audio_coreaudio.m");
    println!("cargo::rerun-if-changed=src/audio_coreaudio.h");
    println!("cargo::rerun-if-changed=src/audio.h");

    cc::Build::new()
        .file(src_dir.join("audio_coreaudio.m"))
        .flag("-fobjc-arc")
        .compile("audio_coreaudio");

    println!("cargo::rustc-link-lib=framework=AudioToolbox");
    println!("cargo::rustc-link-lib=framework=CoreAudio");
    println!("cargo::rustc-link-lib=framework=Foundation");
}

fn build_linux_pulse(src_dir: &Path) {
    println!("cargo::rerun-if-changed=src/audio_pulse.c");
    println!("cargo::rerun-if-changed=src/audio_pulse.h");
    println!("cargo::rerun-if-changed=src/audio.h");

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
    println!("cargo::rerun-if-changed=src/audio_pipewire.h");
    println!("cargo::rerun-if-changed=src/audio.h");

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

fn generate_bindings(builder: Builder, out_file: &str, out_dir: &Path) {
    let bindings = builder
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

    bindings
        .write_to_file(out_dir.join(out_file))
        .expect("Failed to write bindings");
}
