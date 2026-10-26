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
#define AUDIO_FORMAT_S16 0  // 符号付き 16-bit 整数
#define AUDIO_FORMAT_F32 1  // 32-bit 浮動小数点数

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

// ---------------------------------------------------------------------------
// セッション API の共通仕様
// ---------------------------------------------------------------------------
//
// 各バックエンドのセッション API は次の仕様に従う。バックエンド固有の
// 戻り値や待機時間は、各バックエンドのヘッダに記載する。
//
// create:
// - device_id が NULL の場合はデフォルトデバイスを使う
// - sample_rate が 0 以下の場合は 48000、channels が 0 以下の場合は
//   キャプチャ 1、再生 2 を使う
// - 成功時は停止状態のセッションを返す。失敗時は NULL を返す
//
// destroy:
// - NULL の場合は何もしない
// - start 前、実行中、start 失敗後、stop 後のいずれでも呼び出せる
// - 実行中なら停止してからリソースを破棄する
// - start / stop / destroy を同時に呼び出してはならない
// - コールバックのスレッド上から呼び出してはならない
//
// start:
// - callback は NULL であってはならない
// - 停止中なら開始する。すでに実行中なら何もせず 0 を返す
// - 成功時は 0、失敗時は負の値を返す。負の値の意味はバックエンド固有
// - コールバックのスレッド上から呼び出してはならない
//
// stop:
// - 停止中なら何もせず 0 を返す
// - 成功時は 0、失敗時は負の値を返す。負の値の意味はバックエンド固有
// - コールバックのスレッド上から呼び出してはならない
// - start / stop / destroy を同時に呼び出してはならない
//
// is_in_callback:
// - 現在のスレッドがそのセッションのユーザーコールバックを実行中なら 1、
//   そうでなければ 0 を返す
// - セッションが NULL の場合は 0 を返す
//
// sample_rate / channels:
// - create で確定した実際の値を返す
// - セッションが NULL の場合は 0 を返す
//
// コールバック:
// - frames、channels、sample_rate、format はその呼び出しの音声形式を表す
// - キャプチャの data はコールバックの実行中だけ有効
// - 再生の buffer はコールバックの実行中だけ有効

#if defined(__cplusplus)
}
#endif
