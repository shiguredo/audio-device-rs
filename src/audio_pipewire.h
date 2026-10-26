#pragma once

#include "audio.h"

#if defined(__cplusplus)
extern "C" {
#endif

// セッション開始の戻り値
enum AudioPipeWireResult {
    AUDIO_PIPEWIRE_RESULT_OK = 0,
    AUDIO_PIPEWIRE_ERROR_INVALID_ARGUMENT = -1,
    AUDIO_PIPEWIRE_ERROR_THREAD_LOOP_START = -2,
    AUDIO_PIPEWIRE_ERROR_CONTEXT_CONNECT = -3,
    AUDIO_PIPEWIRE_ERROR_PROPERTIES = -4,
    AUDIO_PIPEWIRE_ERROR_STREAM_CREATE = -5,
    AUDIO_PIPEWIRE_ERROR_STREAM_CONNECT = -6,
    AUDIO_PIPEWIRE_ERROR_CLOCK = -7,
    AUDIO_PIPEWIRE_ERROR_WAIT = -8,
    AUDIO_PIPEWIRE_ERROR_TIMEOUT = -9,
    AUDIO_PIPEWIRE_ERROR_STREAM_STATE = -10,
    AUDIO_PIPEWIRE_ERROR_SET_ACTIVE = -11,
    AUDIO_PIPEWIRE_ERROR_CORE = -12,
};

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
//
// start は 0 で成功、負で失敗する。負の値は AudioPipeWireResult に対応する。
// stop はコールバックのスレッド上から呼び出してはならない。呼び出し側は
// audio_pipewire_session_is_in_callback で確認する。
struct AudioSession* audio_pipewire_session_create(const char* device_id, int sample_rate, int channels);
void audio_pipewire_session_destroy(struct AudioSession* session);
int audio_pipewire_session_is_in_callback(struct AudioSession* session);
int audio_pipewire_session_start(struct AudioSession* session, AudioFrameCallback callback, void* user_data);
int audio_pipewire_session_stop(struct AudioSession* session);
int audio_pipewire_session_sample_rate(struct AudioSession* session);
int audio_pipewire_session_channels(struct AudioSession* session);

// 再生セッション管理
//
// start は 0 で成功、負で失敗する。負の値は AudioPipeWireResult に対応する。
// stop はコールバックのスレッド上から呼び出してはならない。呼び出し側は
// audio_pipewire_playback_session_is_in_callback で確認する。
struct PlaybackSession* audio_pipewire_playback_session_create(const char* device_id, int sample_rate, int channels);
void audio_pipewire_playback_session_destroy(struct PlaybackSession* session);
int audio_pipewire_playback_session_is_in_callback(struct PlaybackSession* session);
int audio_pipewire_playback_session_start(struct PlaybackSession* session, AudioPlaybackCallback callback, void* user_data);
int audio_pipewire_playback_session_stop(struct PlaybackSession* session);
int audio_pipewire_playback_session_sample_rate(struct PlaybackSession* session);
int audio_pipewire_playback_session_channels(struct PlaybackSession* session);

#if defined(__cplusplus)
}
#endif
