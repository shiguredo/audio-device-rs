//! C / Objective-C バックエンドの FFI バインディング。
//!
//! `build.rs` が `bindgen` で生成したバインディングを `include!` で取り込む。
//! バックエンドごとに読み込むファイルを分けている。

#[cfg(enable_coreaudio)]
include!(concat!(env!("OUT_DIR"), "/bindings_coreaudio.rs"));

#[cfg(any(enable_pulse, enable_pipewire))]
include!(concat!(env!("OUT_DIR"), "/bindings_linux.rs"));
