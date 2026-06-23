#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

#[cfg(enable_coreaudio)]
include!(concat!(env!("OUT_DIR"), "/bindings_coreaudio.rs"));

#[cfg(any(enable_pulse, enable_pipewire))]
include!(concat!(env!("OUT_DIR"), "/bindings_linux.rs"));
