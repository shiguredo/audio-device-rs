//! macOS / Linux 共通のオーディオデバイス実装。
//!
//! バックエンドごとに FFI 関数群を [DeviceOps] で渡し、[FfiDeviceImpl] と
//! [FfiDeviceListImpl] がデバイス情報取得 / デバイス列挙を一括で提供する。

use std::ffi::{CStr, c_char};
use std::ptr::NonNull;

use crate::common::AudioDeviceType;
use crate::error::{Error, Result};
use crate::ffi;

// ---------------------------------------------------------------------------
// バックエンドとの境界
// ---------------------------------------------------------------------------

/// バックエンド固有の FFI 関数テーブル。
struct DeviceOps {
    pub device_name: unsafe extern "C" fn(device: *mut ffi::AudioDevice) -> *const c_char,
    pub device_unique_id: unsafe extern "C" fn(device: *mut ffi::AudioDevice) -> *const c_char,
    pub device_channels: unsafe extern "C" fn(device: *mut ffi::AudioDevice) -> i32,
    pub device_sample_rate: unsafe extern "C" fn(device: *mut ffi::AudioDevice) -> i32,
    pub device_type: unsafe extern "C" fn(device: *mut ffi::AudioDevice) -> i32,
    pub enumerate_devices:
        unsafe extern "C" fn(devices_ptr: *mut *mut *mut ffi::AudioDevice, count: *mut i32) -> i32,
    pub free_devices: unsafe extern "C" fn(devices_ptr: *mut *mut ffi::AudioDevice, count: i32),
}

// ---------------------------------------------------------------------------
// 単一デバイス
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub(crate) struct FfiDeviceImpl {
    ops: &'static DeviceOps,
    raw: NonNull<ffi::AudioDevice>,
    device_type: AudioDeviceType,
}

impl FfiDeviceImpl {
    pub fn name(&self) -> Result<String> {
        let name_ptr = unsafe { (self.ops.device_name)(self.raw.as_ptr()) };
        if name_ptr.is_null() {
            return Err(Error::NullPointer("device name"));
        }
        let name = unsafe { CStr::from_ptr(name_ptr) };
        Ok(name.to_string_lossy().into_owned())
    }

    pub fn unique_id(&self) -> Result<String> {
        let id_ptr = unsafe { (self.ops.device_unique_id)(self.raw.as_ptr()) };
        if id_ptr.is_null() {
            return Err(Error::NullPointer("device unique_id"));
        }
        let id = unsafe { CStr::from_ptr(id_ptr) };
        Ok(id.to_string_lossy().into_owned())
    }

    pub fn channels(&self) -> i32 {
        unsafe { (self.ops.device_channels)(self.raw.as_ptr()) }
    }

    pub fn sample_rate(&self) -> i32 {
        unsafe { (self.ops.device_sample_rate)(self.raw.as_ptr()) }
    }

    pub fn device_type(&self) -> AudioDeviceType {
        self.device_type
    }
}

// SAFETY: 内部に保持する FFI デバイスポインタは C 側のスレッド安全性に従う。
// 全バックエンドでデバイス情報は read-only であり、複数スレッドからの参照は安全。
unsafe impl Send for FfiDeviceImpl {}
unsafe impl Sync for FfiDeviceImpl {}

// ---------------------------------------------------------------------------
// デバイスリスト（列挙・解放を内包）
// ---------------------------------------------------------------------------

pub(crate) struct FfiDeviceListImpl {
    ops: &'static DeviceOps,
    devices_ptr: *mut *mut ffi::AudioDevice,
    count: i32,
    devices: Vec<FfiDeviceImpl>,
}

impl FfiDeviceListImpl {
    fn enumerate(ops: &'static DeviceOps, filter: Option<AudioDeviceType>) -> Result<Self> {
        let mut devices_ptr: *mut *mut ffi::AudioDevice = std::ptr::null_mut();
        let mut count: i32 = 0;

        let ret = unsafe { (ops.enumerate_devices)(&mut devices_ptr, &mut count) };
        if ret < 0 || devices_ptr.is_null() {
            return Err(Error::DeviceAccessDenied);
        }

        let devices: Vec<FfiDeviceImpl> = (0..count as usize)
            .filter_map(|i| {
                let device_ptr = unsafe { *devices_ptr.add(i) };
                NonNull::new(device_ptr).and_then(|raw| {
                    let device_type =
                        AudioDeviceType::from_ffi(unsafe { (ops.device_type)(raw.as_ptr()) })
                            .ok()?;
                    let device = FfiDeviceImpl {
                        ops,
                        raw,
                        device_type,
                    };
                    if filter.is_none_or(|f| device.device_type == f) {
                        Some(device)
                    } else {
                        None
                    }
                })
            })
            .collect();

        Ok(Self {
            ops,
            devices_ptr,
            count,
            devices,
        })
    }

    #[cfg(enable_coreaudio)]
    pub(crate) fn enumerate_coreaudio(filter: Option<AudioDeviceType>) -> Result<Self> {
        Self::enumerate(&OPS_COREAUDIO, filter)
    }

    #[cfg(enable_pulse)]
    pub(crate) fn enumerate_pulse(filter: Option<AudioDeviceType>) -> Result<Self> {
        Self::enumerate(&OPS_PULSE, filter)
    }

    #[cfg(enable_pipewire)]
    pub(crate) fn enumerate_pipewire(filter: Option<AudioDeviceType>) -> Result<Self> {
        Self::enumerate(&OPS_PIPEWIRE, filter)
    }

    pub fn as_slice(&self) -> &[FfiDeviceImpl] {
        &self.devices
    }
}

impl Drop for FfiDeviceListImpl {
    fn drop(&mut self) {
        if !self.devices_ptr.is_null() {
            unsafe { (self.ops.free_devices)(self.devices_ptr, self.count) };
        }
    }
}

// SAFETY: 内部の FFI ポインタ配列は read-only 参照であり、スレッド間共有は安全。
unsafe impl Send for FfiDeviceListImpl {}
unsafe impl Sync for FfiDeviceListImpl {}

// ---------------------------------------------------------------------------
// バックエンド別 OPS 定数
// ---------------------------------------------------------------------------

#[cfg(enable_coreaudio)]
const OPS_COREAUDIO: DeviceOps = DeviceOps {
    device_name: ffi::audio_coreaudio_device_name,
    device_unique_id: ffi::audio_coreaudio_device_unique_id,
    device_channels: ffi::audio_coreaudio_device_channels,
    device_sample_rate: ffi::audio_coreaudio_device_sample_rate,
    device_type: ffi::audio_coreaudio_device_type,
    enumerate_devices: ffi::audio_coreaudio_enumerate_devices,
    free_devices: ffi::audio_coreaudio_free_devices,
};

#[cfg(enable_pulse)]
const OPS_PULSE: DeviceOps = DeviceOps {
    device_name: ffi::audio_pulse_device_name,
    device_unique_id: ffi::audio_pulse_device_unique_id,
    device_channels: ffi::audio_pulse_device_channels,
    device_sample_rate: ffi::audio_pulse_device_sample_rate,
    device_type: ffi::audio_pulse_device_type,
    enumerate_devices: ffi::audio_pulse_enumerate_devices,
    free_devices: ffi::audio_pulse_free_devices,
};

#[cfg(enable_pipewire)]
const OPS_PIPEWIRE: DeviceOps = DeviceOps {
    device_name: ffi::audio_pipewire_device_name,
    device_unique_id: ffi::audio_pipewire_device_unique_id,
    device_channels: ffi::audio_pipewire_device_channels,
    device_sample_rate: ffi::audio_pipewire_device_sample_rate,
    device_type: ffi::audio_pipewire_device_type,
    enumerate_devices: ffi::audio_pipewire_enumerate_devices,
    free_devices: ffi::audio_pipewire_free_devices,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::AudioFormat;

    #[test]
    fn device_type_from_ffi_known_values() {
        assert_eq!(
            AudioDeviceType::from_ffi(ffi::AUDIO_DEVICE_TYPE_INPUT as i32).unwrap(),
            AudioDeviceType::Input
        );
        assert_eq!(
            AudioDeviceType::from_ffi(ffi::AUDIO_DEVICE_TYPE_OUTPUT as i32).unwrap(),
            AudioDeviceType::Output
        );
    }

    #[test]
    fn device_type_from_ffi_unknown_values() {
        assert!(AudioDeviceType::from_ffi(-1).is_err());
        assert!(AudioDeviceType::from_ffi(2).is_err());
        assert!(AudioDeviceType::from_ffi(i32::MAX).is_err());
        assert!(AudioDeviceType::from_ffi(i32::MIN).is_err());
    }

    #[test]
    fn audio_format_from_ffi_known_values() {
        assert_eq!(
            AudioFormat::from_ffi(ffi::AUDIO_FORMAT_S16 as i32).unwrap(),
            AudioFormat::S16
        );
        assert_eq!(
            AudioFormat::from_ffi(ffi::AUDIO_FORMAT_F32 as i32).unwrap(),
            AudioFormat::F32
        );
    }

    #[test]
    fn audio_format_from_ffi_unknown_values() {
        assert!(AudioFormat::from_ffi(-1).is_err());
        assert!(AudioFormat::from_ffi(2).is_err());
        assert!(AudioFormat::from_ffi(i32::MAX).is_err());
        assert!(AudioFormat::from_ffi(i32::MIN).is_err());
    }
}
