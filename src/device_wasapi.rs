//! Windows 用オーディオデバイス列挙 (WASAPI)

use std::marker::PhantomData;

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

// Safety: COM オブジェクトは MTA (COINIT_MULTITHREADED) で初期化しており、
// MTA オブジェクトはスレッド間で安全に移送できる。
// 各具体型に対する unsafe impl Send は、利用側（capture_wasapi.rs, playback_wasapi.rs）で宣言する。
impl<T> SendPtr<T> {
    pub(crate) fn into_inner(self) -> T {
        self.0
    }
}

/// 呼び出し元スレッドの COM 初期化 (MTA への参加) に対応する RAII ガード。
pub(crate) struct ComGuard {
    /// このガードを `!Send` / `!Sync` にするためのマーカー。値は保持しない。
    _not_send: PhantomData<*const ()>,
}

impl ComGuard {
    /// COM を MTA モードで初期化し、初期化カウンタを解放するガードを返す。
    ///
    /// 既に MTA で初期化済み (S_FALSE) の場合も成功とし、その場合も `CoInitializeEx` が
    /// カウンタを 1 増やすため、返されたガードを drop する必要がある。
    /// 呼び出し元スレッドが STA で初期化済みの場合は RPC_E_CHANGED_MODE が返るため、
    /// エラーとして報告する。
    pub(crate) fn init_mta() -> Result<Self> {
        // Safety: CoInitializeEx は成功時にスレッドの COM 初期化カウンタを 1 増やす。
        // その解放は返したガードの drop で行う。
        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if hr.is_ok() {
                Ok(Self {
                    _not_send: PhantomData,
                })
            } else {
                Err(Error::ComInitFailed)
            }
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        // Safety: このガードは CoInitializeEx が成功したときにだけ生成される。
        // !Send なので生成したスレッド以外では drop されず、ここで呼ぶ CoUninitialize は
        // 対応する CoInitializeEx と同じスレッドになる。
        unsafe {
            CoUninitialize();
        }
    }
}

/// COM の呼び出しエラーをクレートのエラーに変換する。
///
/// 呼び出し元スレッドが COM を初期化していない場合は、原因が分かるように
/// `ComInitFailed` にする。
fn map_com_error(error: &windows::core::Error, fallback: Error) -> Error {
    if error.code() == CO_E_NOTINITIALIZED {
        Error::ComInitFailed
    } else {
        fallback
    }
}

/// オーディオデバイス
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

/// オーディオデバイスリスト
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
    unsafe {
        // デバイス列挙子を作成
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| map_com_error(&e, Error::DeviceAccessDenied))?;

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

/// デバイス名を取得
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

    // WAVE_FORMAT_IEEE_FLOAT の場合は F32 と判定する
    if format_tag == WAVE_FORMAT_IEEE_FLOAT as u16 {
        return AudioFormat::F32;
    }

    // WAVE_FORMAT_EXTENSIBLE の場合は SubFormat GUID を確認する
    if format_tag == WAVE_FORMAT_EXTENSIBLE as u16 {
        let ext = wave_format as *const WAVEFORMATEXTENSIBLE;
        let sub_format = unsafe { std::ptr::addr_of!((*ext).SubFormat).read_unaligned() };
        if sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
            return AudioFormat::F32;
        }
    }

    // それ以外は全て S16 として扱う
    AudioFormat::S16
}

/// デバイス ID からデバイスを取得する。
pub(crate) fn get_device_by_id(
    device_id: Option<&str>,
    device_type: AudioDeviceType,
) -> Result<IMMDevice> {
    unsafe {
        // デバイス列挙子を作成
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| map_com_error(&e, Error::DeviceAccessDenied))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 呼び出し元スレッドの COM 初期化カウンタが 0 であることを検証する。
    ///
    /// カウンタが 0 なら `CoInitializeEx` は新規初期化になって S_OK を返す。
    /// 0 でなければ S_FALSE が返るため、カウンタが解放されていないことを検出できる。
    fn assert_com_initialization_count_is_zero() {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            assert_eq!(
                hr, S_OK,
                "COM 初期化カウンタが 0 に戻っていれば新規初期化になり S_OK が返ること"
            );
            // 検証のために増やしたカウンタを解放する。
            CoUninitialize();
        }
    }

    /// `ComGuard::init_mta` が返すガードを drop すると、スレッドの COM 初期化カウンタが
    /// 初期化前の状態に戻ること。
    #[test]
    fn com_guard_decrements_thread_initialization_count() {
        // テストスレッドは COM 未初期化の状態で始まるため、最初の初期化は S_OK になる。
        let guard =
            ComGuard::init_mta().expect("テストスレッドで COM を MTA モードで初期化できること");

        unsafe {
            // ガードを保持している間はスレッドが既に MTA で初期化済みなので S_FALSE が返る。
            // この呼び出しでもカウンタが 1 増えるため、ここで対応する CoUninitialize を呼ぶ。
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            assert_eq!(
                hr, S_FALSE,
                "ガードを保持している間はスレッドが MTA で初期化済みとして扱われること"
            );
            CoUninitialize();
        }

        // ガードを drop すると、ComGuard::init_mta が増やしたカウンタが 1 減る。
        drop(guard);
        assert_com_initialization_count_is_zero();
    }

    /// デバイス列挙が呼び出し元スレッドの COM 初期化を必要とすること。
    #[test]
    fn enumerate_devices_requires_thread_initialization() {
        // 初期化していないスレッドでは COM が使えないため、列挙は ComInitFailed で失敗する。
        assert!(
            matches!(
                WasapiDeviceListImpl::enumerate(None),
                Err(Error::ComInitFailed)
            ),
            "COM を初期化していないスレッドでは列挙が ComInitFailed で失敗すること"
        );

        // 初期化している間は列挙でき、ガードを drop するとカウンタが元に戻る。
        // デバイスの有無によって列挙の成否は変わるため、結果は破棄する。
        let guard = crate::initialize_thread()
            .expect("テストスレッドで COM を MTA モードで初期化できること");
        let _ = WasapiDeviceListImpl::enumerate(None);
        drop(guard);
        assert_com_initialization_count_is_zero();
    }
}
