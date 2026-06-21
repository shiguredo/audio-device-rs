//! Windows 用オーディオデバイス列挙 (WASAPI)

use windows::{
    Win32::Devices::FunctionDiscovery::*, Win32::Media::Audio::*,
    Win32::Media::KernelStreaming::*, Win32::Media::Multimedia::*,
    Win32::System::Com::StructuredStorage::*, Win32::System::Com::*, Win32::System::Variant::*,
    Win32::UI::Shell::PropertiesSystem::*, Win32::Foundation::*, core::*,
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

// Safety: COM オブジェクトは MTA (COINIT_MULTITHREADED) で初期化しており、
// MTA オブジェクトはスレッド間で安全に移送できる。
// 各具体型に対する unsafe impl Send は、利用側（capture_windows.rs, playback_windows.rs）で宣言する。

impl<T> SendPtr<T> {
    pub(crate) fn into_inner(self) -> T {
        self.0
    }
}

/// COM を MTA モードで初期化する。
/// 既に MTA で初期化済み (S_FALSE) の場合は成功とする。
/// 呼び出し元スレッドが STA で初期化済みの場合は RPC_E_CHANGED_MODE が返るため、
/// エラーとして報告する。
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

#[derive(Debug)]
/// オーディオデバイス
pub struct AudioDevice {
    name: String,
    unique_id: String,
    channels: i32,
    sample_rate: i32,
    device_type: AudioDeviceType,
}

impl AudioDevice {
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

unsafe impl Send for AudioDevice {}
unsafe impl Sync for AudioDevice {}

/// オーディオデバイスリスト
pub struct AudioDeviceList {
    devices: Vec<AudioDevice>,
}

impl AudioDeviceList {
    /// 入力デバイス（マイク）を列挙する
    ///
    /// 既存の `enumerate()` と同等の動作
    pub fn enumerate_input() -> Result<Self> {
        let devices = enumerate_devices_by_type(AudioDeviceType::Input)?;
        Ok(Self { devices })
    }

    /// 出力デバイス（スピーカー）を列挙する
    pub fn enumerate_output() -> Result<Self> {
        let devices = enumerate_devices_by_type(AudioDeviceType::Output)?;
        Ok(Self { devices })
    }

    /// 全デバイス（入力・出力）を列挙する
    pub fn enumerate() -> Result<Self> {
        let mut devices = enumerate_devices_by_type(AudioDeviceType::Input)?;
        devices.extend(enumerate_devices_by_type(AudioDeviceType::Output)?);
        Ok(Self { devices })
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

unsafe impl Send for AudioDeviceList {}
unsafe impl Sync for AudioDeviceList {}

/// 指定されたタイプのデバイスを列挙
fn enumerate_devices_by_type(device_type: AudioDeviceType) -> Result<Vec<AudioDevice>> {
    init_com_mta()?;
    unsafe {
        // デバイス列挙子を作成
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|_| Error::DeviceAccessDenied)?;

        // デバイスタイプに応じてデータフローを選択
        let data_flow = match device_type {
            AudioDeviceType::Input => eCapture,
            AudioDeviceType::Output => eRender,
        };

        // デバイスを列挙
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

            // デバイス ID を取得
            let device_id = match device.GetId() {
                Ok(id) => id.to_string().unwrap_or_default(),
                Err(_) => continue,
            };

            // デバイスプロパティを取得
            let props = device.OpenPropertyStore(STGM_READ);
            let name = if let Ok(props) = props {
                get_device_name(&props).unwrap_or_else(|| "Unknown Device".to_string())
            } else {
                "Unknown Device".to_string()
            };

            // フォーマット情報を取得
            let (channels, sample_rate) = get_device_format(&device).unwrap_or((2, 48000));

            devices.push(AudioDevice {
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

/// デバイス名を取得
fn get_device_name(props: &IPropertyStore) -> Option<String> {
    unsafe {
        let mut prop_value = props.GetValue(&PKEY_Device_FriendlyName).ok()?;

        if prop_value.Anonymous.Anonymous.vt == VT_LPWSTR {
            let pwsz = prop_value.Anonymous.Anonymous.Anonymous.pwszVal;
            if !pwsz.is_null() {
                let len = (0..).take_while(|&i| *pwsz.0.add(i) != 0).count();
                let slice = std::slice::from_raw_parts(pwsz.0, len);
                let name = String::from_utf16(slice).ok();
                PropVariantClear(&mut prop_value).ok();
                return name;
            }
        }
        PropVariantClear(&mut prop_value).ok();
        None
    }
}

/// デバイスのフォーマット情報を取得
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

/// オーディオフォーマットを判定
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

/// デバイス ID からデバイスを取得
pub(crate) fn get_device_by_id(
    device_id: Option<&str>,
    device_type: AudioDeviceType,
) -> Result<IMMDevice> {
    init_com_mta()?;
    unsafe {
        // デバイス列挙子を作成
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|_| Error::DeviceAccessDenied)?;

        if let Some(id) = device_id {
            // 指定されたデバイスを取得
            let wide_id: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
            let pcwstr = PCWSTR::from_raw(wide_id.as_ptr());
            enumerator
                .GetDevice(pcwstr)
                .map_err(|_| Error::DeviceNotFound)
        } else {
            // デフォルトデバイスを取得
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
