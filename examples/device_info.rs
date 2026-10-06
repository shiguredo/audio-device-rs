//! デバイスごとの詳細情報と統計情報を JSON で出力するサンプル。
//!
//! オーディオバックエンド (coreaudio / pulse / pipewire / wasapi) のいずれかが
//! 有効な場合に動作する。

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi))]
fn main() {
    use std::collections::BTreeSet;

    use shiguredo_audio_device::{AudioDeviceList, AudioDeviceType};

    let device_list = match AudioDeviceList::enumerate() {
        Ok(list) => list,
        Err(e) => {
            eprintln!("Failed to enumerate devices: {e}");
            std::process::exit(1);
        }
    };

    let mut input_count: usize = 0;
    let mut output_count: usize = 0;
    let mut sample_rates: BTreeSet<i32> = BTreeSet::new();
    let mut channel_counts: BTreeSet<i32> = BTreeSet::new();

    for device in &device_list {
        match device.device_type() {
            AudioDeviceType::Input => input_count += 1,
            AudioDeviceType::Output => output_count += 1,
        }
        sample_rates.insert(device.sample_rate());
        channel_counts.insert(device.channels());
    }

    let output = nojson::json(|f| {
        f.set_indent_size(2);
        f.set_spacing(true);
        f.object(|f| {
            f.member(
                "devices",
                nojson::array(|f| {
                    for device in &device_list {
                        let name = device.name().unwrap_or_default();
                        let unique_id = device.unique_id().unwrap_or_default();
                        let device_type = match device.device_type() {
                            AudioDeviceType::Input => "input",
                            AudioDeviceType::Output => "output",
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
            )?;
            f.member(
                "statistics",
                nojson::object(|f| {
                    f.member("device_count", device_list.len())?;
                    f.member("input_count", input_count)?;
                    f.member("output_count", output_count)?;
                    f.member(
                        "sample_rates",
                        nojson::array(|f| {
                            for rate in &sample_rates {
                                f.element(*rate)?;
                            }
                            Ok(())
                        }),
                    )?;
                    f.member(
                        "channel_counts",
                        nojson::array(|f| {
                            for ch in &channel_counts {
                                f.element(*ch)?;
                            }
                            Ok(())
                        }),
                    )
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
