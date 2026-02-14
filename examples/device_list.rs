use shiguredo_audio_device::AudioDeviceList;

fn main() {
    let device_list = match AudioDeviceList::enumerate() {
        Ok(list) => list,
        Err(e) => {
            eprintln!("デバイスの列挙に失敗しました: {e}");
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
                    for device in device_list.devices() {
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
