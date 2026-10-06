//! 全てのオーディオデバイスを JSON で出力するサンプル。
//!
//! オーディオバックエンド (coreaudio / pulse / pipewire / wasapi) のいずれかが
//! 有効な場合に動作する。

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi))]
fn main() {
    use shiguredo_audio_device::AudioDeviceList;

    let device_list = match AudioDeviceList::enumerate() {
        Ok(list) => list,
        Err(e) => {
            eprintln!("Failed to enumerate devices: {e}");
            std::process::exit(1);
        }
    };

    let output = nojson::json(|f| {
        f.set_indent_size(2);
        f.set_spacing(true);
        f.object(|f| {
            f.member("device_count", device_list.len())?;
            f.member(
                "devices",
                nojson::array(|f| {
                    for device in &device_list {
                        let name = device.name().unwrap_or_default();
                        let unique_id = device.unique_id().unwrap_or_default();
                        let device_type = match device.device_type() {
                            shiguredo_audio_device::AudioDeviceType::Input => "input",
                            shiguredo_audio_device::AudioDeviceType::Output => "output",
                        };
                        f.element(nojson::object(|f| {
                            f.member("name", name.as_str())?;
                            f.member("unique_id", unique_id.as_str())?;
                            f.member("device_type", device_type)?;
                            f.member("channels", device.channels())?;
                            f.member("sample_rate", device.sample_rate())
                        }))?;
                    }
                    Ok(())
                }),
            )
        })
    });

    println!("{output}");
}

/// オーディオバックエンドが 1 つも有効でない場合はエラー終了する
///
/// `--no-default-features` のみでビルドした場合にこの main が選ばれる。
#[cfg(not(any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi)))]
fn main() {
    eprintln!(
        "No audio backend feature is enabled. Enable one of coreaudio, pulse, pipewire, or wasapi."
    );
    std::process::exit(1);
}
