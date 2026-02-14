#pragma once

#include <stdint.h>

#if defined(__cplusplus)
extern "C" {
#endif

struct AudioDevice;
struct AudioSession;

// オーディオフォーマット定数
#define AUDIO_FORMAT_S16 0  // signed 16-bit integer
#define AUDIO_FORMAT_F32 1  // 32-bit float

// デバイスタイプ定数
#define AUDIO_DEVICE_TYPE_INPUT  0  // 入力デバイス（マイク）
#define AUDIO_DEVICE_TYPE_OUTPUT 1  // 出力デバイス（スピーカー）

// フレームコールバック
// data: PCM データ（S16 または F32）
// frames: サンプルフレーム数
// channels: チャンネル数
// sample_rate: サンプルレート
// format: AUDIO_FORMAT_S16 または AUDIO_FORMAT_F32
// timestamp_us: タイムスタンプ（マイクロ秒）
typedef void (*AudioFrameCallback)(void* user_data,
                                    const void* data,
                                    int frames,
                                    int channels,
                                    int sample_rate,
                                    int format,
                                    int64_t timestamp_us);

// デバイス列挙
// 成功時は 0、失敗時は負の値を返す
int audio_enumerate_devices(struct AudioDevice*** devices, int* count);

// デバイス配列を解放
void audio_free_devices(struct AudioDevice** devices, int count);

// デバイス名を取得（NULL 終端文字列）
const char* audio_device_name(struct AudioDevice* device);

// デバイスの一意識別子を取得（NULL 終端文字列）
const char* audio_device_unique_id(struct AudioDevice* device);

// デバイスのチャンネル数を取得
int audio_device_channels(struct AudioDevice* device);

// デバイスのサンプルレートを取得
int audio_device_sample_rate(struct AudioDevice* device);

// デバイスのタイプを取得（AUDIO_DEVICE_TYPE_INPUT または AUDIO_DEVICE_TYPE_OUTPUT）
int audio_device_type(struct AudioDevice* device);

// セッションを作成
// device_id が NULL の場合はデフォルトデバイスを使用
// sample_rate が 0 の場合はデバイスのデフォルトを使用
// channels が 0 の場合はデバイスのデフォルトを使用
struct AudioSession* audio_session_create(const char* device_id,
                                           int sample_rate,
                                           int channels);

// セッションを破棄
void audio_session_destroy(struct AudioSession* session);

// キャプチャを開始
// 成功時は 0、失敗時は負の値を返す
int audio_session_start(struct AudioSession* session,
                         AudioFrameCallback callback,
                         void* user_data);

// キャプチャを停止
void audio_session_stop(struct AudioSession* session);

// セッションのサンプルレートを取得
int audio_session_sample_rate(struct AudioSession* session);

// セッションのチャンネル数を取得
int audio_session_channels(struct AudioSession* session);

#if defined(__cplusplus)
}
#endif
