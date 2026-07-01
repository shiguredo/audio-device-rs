//! オーディオデバイスとデバイスリストの定義。
//!
//! バックエンドに依存しない形でデバイス情報（名前、一意識別子、チャンネル数、サンプルレート）を
//! 取得する [AudioDevice] と、デバイス列挙の [AudioDeviceList] を提供する。

use crate::common::AudioDeviceType;
use crate::error::Result;

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
use crate::device_ffi::{FfiDeviceImpl, FfiDeviceListImpl};

#[cfg(enable_wasapi)]
use crate::device_wasapi::{WasapiDeviceImpl, WasapiDeviceListImpl};

/// オーディオデバイス。
pub struct AudioDevice(pub(crate) AudioDeviceInner);

pub(crate) enum AudioDeviceInner {
    #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
    Ffi(FfiDeviceImpl),
    #[cfg(enable_wasapi)]
    Wasapi(WasapiDeviceImpl),
}

impl AudioDevice {
    /// デバイス名を取得する。
    pub fn name(&self) -> Result<String> {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioDeviceInner::Ffi(inner) => inner.name(),
            #[cfg(enable_wasapi)]
            AudioDeviceInner::Wasapi(inner) => inner.name(),
        }
    }

    /// デバイスの一意識別子を取得する。
    pub fn unique_id(&self) -> Result<String> {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioDeviceInner::Ffi(inner) => inner.unique_id(),
            #[cfg(enable_wasapi)]
            AudioDeviceInner::Wasapi(inner) => inner.unique_id(),
        }
    }

    /// チャンネル数を取得する。
    pub fn channels(&self) -> i32 {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioDeviceInner::Ffi(inner) => inner.channels(),
            #[cfg(enable_wasapi)]
            AudioDeviceInner::Wasapi(inner) => inner.channels(),
        }
    }

    /// サンプルレートを取得する。
    pub fn sample_rate(&self) -> i32 {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioDeviceInner::Ffi(inner) => inner.sample_rate(),
            #[cfg(enable_wasapi)]
            AudioDeviceInner::Wasapi(inner) => inner.sample_rate(),
        }
    }

    /// デバイスの種類を取得する。
    pub fn device_type(&self) -> AudioDeviceType {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioDeviceInner::Ffi(inner) => inner.device_type(),
            #[cfg(enable_wasapi)]
            AudioDeviceInner::Wasapi(inner) => inner.device_type(),
        }
    }
}

unsafe impl Send for AudioDevice {}
unsafe impl Sync for AudioDevice {}

/// オーディオデバイスリスト。
pub struct AudioDeviceList(pub(crate) AudioDeviceListInner);

pub(crate) enum AudioDeviceListInner {
    #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
    Ffi {
        _inner: FfiDeviceListImpl,
        devices: Vec<AudioDevice>,
    },
    #[cfg(enable_wasapi)]
    Wasapi {
        _inner: WasapiDeviceListImpl,
        devices: Vec<AudioDevice>,
    },
}

impl AudioDeviceList {
    /// デフォルトバックエンドでデバイスを列挙する。
    #[cfg(any(
        enable_default_coreaudio,
        enable_default_pulse,
        enable_default_pipewire,
        enable_default_wasapi
    ))]
    pub fn enumerate() -> Result<Self> {
        #[cfg(enable_default_coreaudio)]
        {
            Self::enumerate_coreaudio()
        }
        #[cfg(enable_default_pulse)]
        {
            Self::enumerate_pulse()
        }
        #[cfg(enable_default_pipewire)]
        {
            Self::enumerate_pipewire()
        }
        #[cfg(enable_default_wasapi)]
        {
            Self::enumerate_wasapi()
        }
    }

    /// デフォルトバックエンドで入力デバイスを列挙する。
    #[cfg(any(
        enable_default_coreaudio,
        enable_default_pulse,
        enable_default_pipewire,
        enable_default_wasapi
    ))]
    pub fn enumerate_input() -> Result<Self> {
        #[cfg(enable_default_coreaudio)]
        {
            Self::enumerate_input_coreaudio()
        }
        #[cfg(enable_default_pulse)]
        {
            Self::enumerate_input_pulse()
        }
        #[cfg(enable_default_pipewire)]
        {
            Self::enumerate_input_pipewire()
        }
        #[cfg(enable_default_wasapi)]
        {
            Self::enumerate_input_wasapi()
        }
    }

    /// デフォルトバックエンドで出力デバイスを列挙する。
    #[cfg(any(
        enable_default_coreaudio,
        enable_default_pulse,
        enable_default_pipewire,
        enable_default_wasapi
    ))]
    pub fn enumerate_output() -> Result<Self> {
        #[cfg(enable_default_coreaudio)]
        {
            Self::enumerate_output_coreaudio()
        }
        #[cfg(enable_default_pulse)]
        {
            Self::enumerate_output_pulse()
        }
        #[cfg(enable_default_pipewire)]
        {
            Self::enumerate_output_pipewire()
        }
        #[cfg(enable_default_wasapi)]
        {
            Self::enumerate_output_wasapi()
        }
    }

    // -----------------------------------------------------------------------
    // CoreAudio 明示関数
    // -----------------------------------------------------------------------

    #[cfg(enable_coreaudio)]
    pub fn enumerate_coreaudio() -> Result<Self> {
        let inner = FfiDeviceListImpl::enumerate_coreaudio(None)?;
        let devices = inner
            .as_slice()
            .iter()
            .map(|d| AudioDevice(AudioDeviceInner::Ffi(*d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Ffi {
            _inner: inner,
            devices,
        }))
    }

    #[cfg(enable_coreaudio)]
    pub fn enumerate_input_coreaudio() -> Result<Self> {
        let inner = FfiDeviceListImpl::enumerate_coreaudio(Some(AudioDeviceType::Input))?;
        let devices = inner
            .as_slice()
            .iter()
            .map(|d| AudioDevice(AudioDeviceInner::Ffi(*d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Ffi {
            _inner: inner,
            devices,
        }))
    }

    #[cfg(enable_coreaudio)]
    pub fn enumerate_output_coreaudio() -> Result<Self> {
        let inner = FfiDeviceListImpl::enumerate_coreaudio(Some(AudioDeviceType::Output))?;
        let devices = inner
            .as_slice()
            .iter()
            .map(|d| AudioDevice(AudioDeviceInner::Ffi(*d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Ffi {
            _inner: inner,
            devices,
        }))
    }

    // -----------------------------------------------------------------------
    // PulseAudio 明示関数
    // -----------------------------------------------------------------------

    #[cfg(enable_pulse)]
    pub fn enumerate_pulse() -> Result<Self> {
        let inner = FfiDeviceListImpl::enumerate_pulse(None)?;
        let devices = inner
            .as_slice()
            .iter()
            .map(|d| AudioDevice(AudioDeviceInner::Ffi(*d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Ffi {
            _inner: inner,
            devices,
        }))
    }

    #[cfg(enable_pulse)]
    pub fn enumerate_input_pulse() -> Result<Self> {
        let inner = FfiDeviceListImpl::enumerate_pulse(Some(AudioDeviceType::Input))?;
        let devices = inner
            .as_slice()
            .iter()
            .map(|d| AudioDevice(AudioDeviceInner::Ffi(*d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Ffi {
            _inner: inner,
            devices,
        }))
    }

    #[cfg(enable_pulse)]
    pub fn enumerate_output_pulse() -> Result<Self> {
        let inner = FfiDeviceListImpl::enumerate_pulse(Some(AudioDeviceType::Output))?;
        let devices = inner
            .as_slice()
            .iter()
            .map(|d| AudioDevice(AudioDeviceInner::Ffi(*d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Ffi {
            _inner: inner,
            devices,
        }))
    }

    // -----------------------------------------------------------------------
    // PipeWire 明示関数
    // -----------------------------------------------------------------------

    #[cfg(enable_pipewire)]
    pub fn enumerate_pipewire() -> Result<Self> {
        let inner = FfiDeviceListImpl::enumerate_pipewire(None)?;
        let devices = inner
            .as_slice()
            .iter()
            .map(|d| AudioDevice(AudioDeviceInner::Ffi(*d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Ffi {
            _inner: inner,
            devices,
        }))
    }

    #[cfg(enable_pipewire)]
    pub fn enumerate_input_pipewire() -> Result<Self> {
        let inner = FfiDeviceListImpl::enumerate_pipewire(Some(AudioDeviceType::Input))?;
        let devices = inner
            .as_slice()
            .iter()
            .map(|d| AudioDevice(AudioDeviceInner::Ffi(*d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Ffi {
            _inner: inner,
            devices,
        }))
    }

    #[cfg(enable_pipewire)]
    pub fn enumerate_output_pipewire() -> Result<Self> {
        let inner = FfiDeviceListImpl::enumerate_pipewire(Some(AudioDeviceType::Output))?;
        let devices = inner
            .as_slice()
            .iter()
            .map(|d| AudioDevice(AudioDeviceInner::Ffi(*d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Ffi {
            _inner: inner,
            devices,
        }))
    }

    // -----------------------------------------------------------------------
    // WASAPI 明示関数
    // -----------------------------------------------------------------------

    #[cfg(enable_wasapi)]
    pub fn enumerate_wasapi() -> Result<Self> {
        let mut inner = WasapiDeviceListImpl::enumerate(None)?;
        let raw_devices = std::mem::take(&mut inner.devices);
        let devices = raw_devices
            .into_iter()
            .map(|d| AudioDevice(AudioDeviceInner::Wasapi(d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Wasapi {
            _inner: inner,
            devices,
        }))
    }

    #[cfg(enable_wasapi)]
    pub fn enumerate_input_wasapi() -> Result<Self> {
        let mut inner = WasapiDeviceListImpl::enumerate(Some(AudioDeviceType::Input))?;
        let raw_devices = std::mem::take(&mut inner.devices);
        let devices = raw_devices
            .into_iter()
            .map(|d| AudioDevice(AudioDeviceInner::Wasapi(d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Wasapi {
            _inner: inner,
            devices,
        }))
    }

    #[cfg(enable_wasapi)]
    pub fn enumerate_output_wasapi() -> Result<Self> {
        let mut inner = WasapiDeviceListImpl::enumerate(Some(AudioDeviceType::Output))?;
        let raw_devices = std::mem::take(&mut inner.devices);
        let devices = raw_devices
            .into_iter()
            .map(|d| AudioDevice(AudioDeviceInner::Wasapi(d)))
            .collect();
        Ok(Self(AudioDeviceListInner::Wasapi {
            _inner: inner,
            devices,
        }))
    }

    // -----------------------------------------------------------------------
    // 共通 API
    // -----------------------------------------------------------------------

    /// デバイスのスライスを取得する。
    pub fn as_slice(&self) -> &[AudioDevice] {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioDeviceListInner::Ffi { devices, .. } => devices,
            #[cfg(enable_wasapi)]
            AudioDeviceListInner::Wasapi { devices, .. } => devices,
        }
    }

    /// デバイス数を取得する。
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// デバイスが空かどうかを返す。
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }
}

impl<'a> IntoIterator for &'a AudioDeviceList {
    type Item = &'a AudioDevice;
    type IntoIter = std::slice::Iter<'a, AudioDevice>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

unsafe impl Send for AudioDeviceList {}
unsafe impl Sync for AudioDeviceList {}
