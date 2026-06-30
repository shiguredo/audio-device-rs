#pragma once

#include <stdint.h>

#if defined(__cplusplus)
extern "C" {
#endif

// 前方宣言
struct AudioDevice;
struct AudioSession;
struct PlaybackSession;

// オーディオフォーマット定数
#define AUDIO_FORMAT_S16 0  // signed 16-bit integer
#define AUDIO_FORMAT_F32 1  // 32-bit float

// デバイスタイプ定数
#define AUDIO_DEVICE_TYPE_INPUT  0  // 入力デバイス（マイク）
#define AUDIO_DEVICE_TYPE_OUTPUT 1  // 出力デバイス（スピーカー）

// フレームコールバック
typedef void (*AudioFrameCallback)(void* user_data,
                                    const void* data,
                                    int frames,
                                    int channels,
                                    int sample_rate,
                                    int format,
                                    int64_t timestamp_us);

// 再生用コールバック
// buffer: 書き込み先バッファ (C 側が確保)
// frames: 要求フレーム数
// channels: チャンネル数
// sample_rate: サンプルレート
// format: AUDIO_FORMAT_S16 または AUDIO_FORMAT_F32
// 戻り値: 実際に書き込んだフレーム数 (0 = 無音)
typedef int (*AudioPlaybackCallback)(void* user_data,
                                     void* buffer,
                                     int frames,
                                     int channels,
                                     int sample_rate,
                                     int format);

#if defined(__cplusplus)
}
#endif
