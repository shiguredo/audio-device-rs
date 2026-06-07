use std::ffi::CStr;
use std::ptr::NonNull;

use crate::error::{Error, Result};
use crate::ffi;

/// オーディオデバイスの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDeviceType {
    /// 入力デバイス（マイク）
    Input,
    /// 出力デバイス（スピーカー）
    Output,
}

impl AudioDeviceType {
    fn from_ffi(device_type: i32) -> Self {
        match device_type {
            x if x == ffi::AUDIO_DEVICE_TYPE_OUTPUT as i32 => AudioDeviceType::Output,
            _ => AudioDeviceType::Input,
        }
    }
}

/// オーディオフォーマット
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    /// Signed 16-bit integer
    S16,
    /// 32-bit float
    F32,
}

impl AudioFormat {
    pub(crate) fn from_ffi(format: i32) -> Self {
        match format {
            x if x == ffi::AUDIO_FORMAT_F32 as i32 => AudioFormat::F32,
            _ => AudioFormat::S16,
        }
    }
}

#[derive(Debug)]
pub struct AudioDevice {
    raw: NonNull<ffi::AudioDevice>,
    device_type: AudioDeviceType,
}

impl AudioDevice {
    pub fn name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::audio_device_name(self.raw.as_ptr()) };
        if name_ptr.is_null() {
            return Err(Error::NullPointer("device name"));
        }
        let name = unsafe { CStr::from_ptr(name_ptr) };
        Ok(name.to_string_lossy().into_owned())
    }

    pub fn unique_id(&self) -> Result<String> {
        let id_ptr = unsafe { ffi::audio_device_unique_id(self.raw.as_ptr()) };
        if id_ptr.is_null() {
            return Err(Error::NullPointer("device unique_id"));
        }
        let id = unsafe { CStr::from_ptr(id_ptr) };
        Ok(id.to_string_lossy().into_owned())
    }

    pub fn channels(&self) -> i32 {
        unsafe { ffi::audio_device_channels(self.raw.as_ptr()) }
    }

    pub fn sample_rate(&self) -> i32 {
        unsafe { ffi::audio_device_sample_rate(self.raw.as_ptr()) }
    }

    pub fn device_type(&self) -> AudioDeviceType {
        self.device_type
    }
}

// AudioDevice は FFI ポインタを持つが、内部データはスレッドセーフ
unsafe impl Send for AudioDevice {}
unsafe impl Sync for AudioDevice {}

pub struct AudioDeviceList {
    devices_ptr: *mut *mut ffi::AudioDevice,
    count: i32,
    devices: Vec<AudioDevice>,
}

impl AudioDeviceList {
    /// 全デバイス（入力・出力）を列挙する
    pub fn enumerate() -> Result<Self> {
        Self::enumerate_internal(None)
    }

    /// 入力デバイス（マイク）を列挙する
    pub fn enumerate_input() -> Result<Self> {
        Self::enumerate_internal(Some(AudioDeviceType::Input))
    }

    /// 出力デバイス（スピーカー）を列挙する
    pub fn enumerate_output() -> Result<Self> {
        Self::enumerate_internal(Some(AudioDeviceType::Output))
    }

    fn enumerate_internal(filter: Option<AudioDeviceType>) -> Result<Self> {
        let mut devices_ptr: *mut *mut ffi::AudioDevice = std::ptr::null_mut();
        let mut count: i32 = 0;

        let ret = unsafe { ffi::audio_enumerate_devices(&mut devices_ptr, &mut count) };
        if ret < 0 {
            return Err(Error::DeviceAccessDenied);
        }

        let devices: Vec<AudioDevice> = if count > 0 && !devices_ptr.is_null() {
            (0..count as usize)
                .filter_map(|i| {
                    let device_ptr = unsafe { *devices_ptr.add(i) };
                    NonNull::new(device_ptr).map(|raw| {
                        let device_type = AudioDeviceType::from_ffi(unsafe {
                            ffi::audio_device_type(raw.as_ptr())
                        });
                        AudioDevice { raw, device_type }
                    })
                })
                .filter(|d| filter.is_none_or(|f| d.device_type == f))
                .collect()
        } else {
            Vec::new()
        };

        Ok(Self {
            devices_ptr,
            count,
            devices,
        })
    }

    pub fn devices(&self) -> &[AudioDevice] {
        &self.devices
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }
}

impl Drop for AudioDeviceList {
    fn drop(&mut self) {
        if !self.devices_ptr.is_null() {
            unsafe {
                ffi::audio_free_devices(self.devices_ptr, self.count);
            }
        }
    }
}

// AudioDeviceList は FFI ポインタを持つが、内部データはスレッドセーフ
unsafe impl Send for AudioDeviceList {}
unsafe impl Sync for AudioDeviceList {}
