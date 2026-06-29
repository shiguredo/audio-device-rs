#pragma once

#include "audio.h"

#if defined(__cplusplus)
extern "C" {
#endif

// デバイス列挙
int audio_pulse_enumerate_devices(struct AudioDevice*** devices, int* count);

// デバイス配列を解放
void audio_pulse_free_devices(struct AudioDevice** devices, int count);

// デバイスアクセサ
const char* audio_pulse_device_name(struct AudioDevice* device);
const char* audio_pulse_device_unique_id(struct AudioDevice* device);
int audio_pulse_device_channels(struct AudioDevice* device);
int audio_pulse_device_sample_rate(struct AudioDevice* device);
int audio_pulse_device_type(struct AudioDevice* device);

// セッション管理
struct AudioSession* audio_pulse_session_create(const char* device_id, int sample_rate, int channels);
void audio_pulse_session_destroy(struct AudioSession* session);
int audio_pulse_session_start(struct AudioSession* session, AudioFrameCallback callback, void* user_data);
void audio_pulse_session_stop(struct AudioSession* session);
int audio_pulse_session_sample_rate(struct AudioSession* session);
int audio_pulse_session_channels(struct AudioSession* session);

#if defined(__cplusplus)
}
#endif
