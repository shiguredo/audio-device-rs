//! Windows 用オーディオデバイス列挙 (WASAPI)

use windows::{
    Win32::Devices::FunctionDiscovery::*, Win32::Foundation::*, Win32::Media::Audio::*,
    Win32::Media::KernelStreaming::*, Win32::Media::Multimedia::*, Win32::System::Com::*,
    Win32::System::Variant::*, Win32::UI::Shell::PropertiesSystem::*, core::*,
};

use crate::common::{AudioDeviceType, AudioFormat};
use crate::error::{Error, Result};

/// Send でない型をスレッドに渡すためのラッパー（MTA で初期化済みのため安全）
pub(crate) struct SendHandle(pub(crate) HANDLE);
unsafe impl Send for SendHandle {}
impl SendHandle {
    pub(crate) fn into_inner(self) -> HANDLE {
        self.0
    }
}

pub(crate) struct SendPtr<T>(pub(crate) T);

impl<T> SendPtr<T> {
    pub(crate) fn into_inner(self) -> T {
        self.0
    }
}

pub(crate) fn init_com_mta() -> Result<()> {
    unsafe {
        let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
        if hr.is_ok() {
            Ok(())
        } else {
            Err(Error::ComInitFailed)
        }
    }
}

pub(crate) struct WasapiDeviceImpl {
    name: String,
    unique_id: String,
    channels: i32,
    sample_rate: i32,
    device_type: AudioDeviceType,
}

impl WasapiDeviceImpl {
    pub fn name(&self) -> Result<String> {
        Ok(self.name.clone())
    }

    pub fn unique_id(&self) -> Result<String> {
        Ok(self.unique_id.clone())
    }

    pub fn channels(&self) -> i32 {
        self.channels
    }

    pub fn sample_rate(&self) -> i32 {
        self.sample_rate
    }

    pub fn device_type(&self) -> AudioDeviceType {
        self.device_type
    }
}

pub(crate) struct WasapiDeviceListImpl {
    pub(crate) devices: Vec<WasapiDeviceImpl>,
}

impl WasapiDeviceListImpl {
    pub fn enumerate(filter: Option<AudioDeviceType>) -> Result<Self> {
        let mut devices = enumerate_devices_by_type(AudioDeviceType::Input)?;
        devices.extend(enumerate_devices_by_type(AudioDeviceType::Output)?);
        if let Some(f) = filter {
            devices.retain(|d| d.device_type == f);
        }
        Ok(Self { devices })
    }
}

fn enumerate_devices_by_type(device_type: AudioDeviceType) -> Result<Vec<WasapiDeviceImpl>> {
    init_com_mta()?;
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|_| Error::DeviceAccessDenied)?;

        let data_flow = match device_type {
            AudioDeviceType::Input => eCapture,
            AudioDeviceType::Output => eRender,
        };

        let collection: IMMDeviceCollection = enumerator
            .EnumAudioEndpoints(data_flow, DEVICE_STATE_ACTIVE)
            .map_err(|_| Error::DeviceAccessDenied)?;

        let count = collection
            .GetCount()
            .map_err(|_| Error::DeviceAccessDenied)?;

        let mut devices = Vec::new();
        for i in 0..count {
            let device = match collection.Item(i) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let device_id = match device.GetId() {
                Ok(id) => id.to_string().unwrap_or_default(),
                Err(_) => continue,
            };
            let props = device.OpenPropertyStore(STGM_READ);
            let name = if let Ok(props) = props {
                get_device_name(&props).unwrap_or_else(|| "Unknown Device".to_string())
            } else {
                "Unknown Device".to_string()
            };
            let (channels, sample_rate) = get_device_format(&device).unwrap_or((2, 48000));
            devices.push(WasapiDeviceImpl {
                name,
                unique_id: device_id,
                channels,
                sample_rate,
                device_type,
            });
        }
        Ok(devices)
    }
}

fn get_device_name(props: &IPropertyStore) -> Option<String> {
    unsafe {
        match props.GetValue(&PKEY_Device_FriendlyName as *const _ as *const _) {
            Ok(pv) => {
                if pv.Anonymous.Anonymous.vt == VARENUM(VT_LPWSTR.0) {
                    let ptr = pv.Anonymous.Anonymous.Anonymous.pwszVal.0;
                    if !ptr.is_null() {
                        let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
                        let slice = std::slice::from_raw_parts(ptr, len);
                        return String::from_utf16(slice).ok();
                    }
                }
                None
            }
            Err(_) => None,
        }
    }
}

fn get_device_format(device: &IMMDevice) -> Option<(i32, i32)> {
    unsafe {
        let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None).ok()?;
        let mix_format = audio_client.GetMixFormat().ok()?;
        let channels = (*mix_format).nChannels as i32;
        let sample_rate = (*mix_format).nSamplesPerSec as i32;
        CoTaskMemFree(Some(mix_format as *const _));
        Some((channels, sample_rate))
    }
}

pub(crate) unsafe fn determine_audio_format(wave_format: *const WAVEFORMATEX) -> AudioFormat {
    let format_tag = unsafe { (*wave_format).wFormatTag };

    if format_tag == WAVE_FORMAT_IEEE_FLOAT as u16 {
        return AudioFormat::F32;
    }

    if format_tag == WAVE_FORMAT_EXTENSIBLE as u16 {
        let ext = wave_format as *const WAVEFORMATEXTENSIBLE;
        let sub_format = unsafe { std::ptr::addr_of!((*ext).SubFormat).read_unaligned() };
        if sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
            return AudioFormat::F32;
        }
    }

    AudioFormat::S16
}

pub(crate) fn get_device_by_id(
    device_id: Option<&str>,
    device_type: AudioDeviceType,
) -> Result<IMMDevice> {
    init_com_mta()?;
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|_| Error::DeviceAccessDenied)?;
        if let Some(id) = device_id {
            let wide_id: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
            let pcwstr = PCWSTR::from_raw(wide_id.as_ptr());
            enumerator
                .GetDevice(pcwstr)
                .map_err(|_| Error::DeviceNotFound)
        } else {
            let data_flow = match device_type {
                AudioDeviceType::Input => eCapture,
                AudioDeviceType::Output => eRender,
            };
            enumerator
                .GetDefaultAudioEndpoint(data_flow, eConsole)
                .map_err(|_| Error::DeviceNotFound)
        }
    }
}
