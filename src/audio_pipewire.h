#pragma once

#include "audio.h"

#if defined(__cplusplus)
extern "C" {
#endif

// デバイス列挙
int audio_pipewire_enumerate_devices(struct AudioDevice*** devices, int* count);

// デバイス配列を解放
void audio_pipewire_free_devices(struct AudioDevice** devices, int count);

// デバイスアクセサ
const char* audio_pipewire_device_name(struct AudioDevice* device);
const char* audio_pipewire_device_unique_id(struct AudioDevice* device);
int audio_pipewire_device_channels(struct AudioDevice* device);
int audio_pipewire_device_sample_rate(struct AudioDevice* device);
int audio_pipewire_device_type(struct AudioDevice* device);

// セッション管理
struct AudioSession* audio_pipewire_session_create(const char* device_id, int sample_rate, int channels);
void audio_pipewire_session_destroy(struct AudioSession* session);
int audio_pipewire_session_start(struct AudioSession* session, AudioFrameCallback callback, void* user_data);
void audio_pipewire_session_stop(struct AudioSession* session);
int audio_pipewire_session_sample_rate(struct AudioSession* session);
int audio_pipewire_session_channels(struct AudioSession* session);

// 再生セッション管理
struct PlaybackSession* audio_pipewire_playback_session_create(const char* device_id, int sample_rate, int channels);
void audio_pipewire_playback_session_destroy(struct PlaybackSession* session);
int audio_pipewire_playback_session_start(struct PlaybackSession* session, AudioPlaybackCallback callback, void* user_data);
void audio_pipewire_playback_session_stop(struct PlaybackSession* session);
int audio_pipewire_playback_session_sample_rate(struct PlaybackSession* session);
int audio_pipewire_playback_session_channels(struct PlaybackSession* session);

#if defined(__cplusplus)
}
#endif
