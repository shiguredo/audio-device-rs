// PulseAudio を使った Linux 用オーディオキャプチャ・再生実装

#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <pulse/pulseaudio.h>

#include "audio_pulse.h"

// タイミング情報の揺らぎを吸収するため、直前のパケットから予測した時刻と
// pa_stream_get_latency() から求めた時刻の差がこの値を超えた場合だけ補正する
#define CAPTURE_TIMESTAMP_RESYNC_US 50000

// AudioDevice 構造体
struct AudioDevice {
    char* name;
    char* unique_id;
    int channels;
    int sample_rate;
    int device_type;
};

// AudioSession 構造体
struct AudioSession {
    pa_threaded_mainloop* mainloop;
    pa_context* context;
    pa_stream* stream;
    char* device_id;
    AudioFrameCallback callback;
    void* user_data;
    int sample_rate;
    int channels;
    atomic_int running;
    // 直前にコールバックへ渡したパケットのタイムスタンプとフレーム数
    int64_t last_timestamp_us;
    int last_frames;
    int has_last_timestamp;
};

// デバイス列挙用のコンテキスト
struct EnumerateContext {
    struct AudioDevice** devices;
    int count;
    int capacity;
    int done;
};

// デバイス列挙コールバック
static void source_info_callback(pa_context* c, const pa_source_info* info,
                                 int eol, void* userdata) {
    (void)c;
    struct EnumerateContext* ctx = userdata;

    if (eol > 0) {
        ctx->done++;
        return;
    }

    if (!info) {
        return;
    }

    // name が NULL のデバイスは識別不能なのでスキップする
    if (!info->name) {
        return;
    }

    // monitor source（スピーカーのモニタ）を除外する
    if (info->monitor_of_sink != PA_INVALID_INDEX) {
        return;
    }

    // 配列を拡張する
    if (ctx->count >= ctx->capacity) {
        int new_capacity = ctx->capacity == 0 ? 8 : ctx->capacity * 2;
        struct AudioDevice** new_devices =
            realloc(ctx->devices, sizeof(struct AudioDevice*) * new_capacity);
        if (!new_devices) {
            return;
        }
        ctx->devices = new_devices;
        ctx->capacity = new_capacity;
    }

    struct AudioDevice* device = calloc(1, sizeof(struct AudioDevice));
    if (!device) {
        return;
    }

    // description を表示名として使用する
    device->name = strdup(info->description ? info->description : info->name);
    if (!device->name) {
        free(device);
        return;
    }

    // PulseAudio の source name を一意識別子として使用する
    device->unique_id = strdup(info->name);
    if (!device->unique_id) {
        free(device->name);
        free(device);
        return;
    }

    device->channels =
        info->sample_spec.channels > 0 ? info->sample_spec.channels : 1;
    device->sample_rate =
        info->sample_spec.rate > 0 ? (int)info->sample_spec.rate : 48000;
    device->device_type = AUDIO_DEVICE_TYPE_INPUT;

    ctx->devices[ctx->count] = device;
    ctx->count++;
}

// 出力デバイス列挙コールバック
static void sink_info_callback(pa_context* c, const pa_sink_info* info,
                                int eol, void* userdata) {
    (void)c;
    struct EnumerateContext* ctx = userdata;

    if (eol > 0) {
        ctx->done++;
        return;
    }

    if (!info) {
        return;
    }

    // name が NULL のデバイスは識別不能なのでスキップする
    if (!info->name) {
        return;
    }

    // 配列を拡張する
    if (ctx->count >= ctx->capacity) {
        int new_capacity = ctx->capacity == 0 ? 8 : ctx->capacity * 2;
        struct AudioDevice** new_devices =
            realloc(ctx->devices, sizeof(struct AudioDevice*) * new_capacity);
        if (!new_devices) {
            return;
        }
        ctx->devices = new_devices;
        ctx->capacity = new_capacity;
    }

    struct AudioDevice* device = calloc(1, sizeof(struct AudioDevice));
    if (!device) {
        return;
    }

    device->name = strdup(info->description ? info->description : info->name);
    if (!device->name) {
        free(device);
        return;
    }

    device->unique_id = strdup(info->name);
    if (!device->unique_id) {
        free(device->name);
        free(device);
        return;
    }

    device->channels =
        info->sample_spec.channels > 0 ? info->sample_spec.channels : 2;
    device->sample_rate =
        info->sample_spec.rate > 0 ? (int)info->sample_spec.rate : 48000;
    device->device_type = AUDIO_DEVICE_TYPE_OUTPUT;

    ctx->devices[ctx->count] = device;
    ctx->count++;
}

// PulseAudio 接続状態コールバック（デバイス列挙用）
static void enumerate_state_callback(pa_context* c, void* userdata) {
    (void)userdata;
    pa_context_state_t state = pa_context_get_state(c);
    // メインループのイテレーションを進めるためにシグナルを送る必要はない
    // pa_mainloop_iterate がブロッキングで処理する
    (void)state;
}

// デバイス列挙
int audio_pulse_enumerate_devices(struct AudioDevice*** devices, int* count) {
    if (!devices || !count) {
        return -1;
    }

    *devices = NULL;
    *count = 0;

    // 同期的にデバイスを列挙するため pa_mainloop を使用する
    pa_mainloop* mainloop = pa_mainloop_new();
    if (!mainloop) {
        return -2;
    }

    pa_mainloop_api* api = pa_mainloop_get_api(mainloop);
    pa_context* context = pa_context_new(api, "shiguredo-audio-enumerate");
    if (!context) {
        pa_mainloop_free(mainloop);
        return -2;
    }

    pa_context_set_state_callback(context, enumerate_state_callback, NULL);

    if (pa_context_connect(context, NULL, PA_CONTEXT_NOFLAGS, NULL) < 0) {
        pa_context_unref(context);
        pa_mainloop_free(mainloop);
        return -3;
    }

    // 接続が完了するまで待機する
    int ret;
    while (pa_context_get_state(context) != PA_CONTEXT_READY) {
        pa_context_state_t state = pa_context_get_state(context);
        if (state == PA_CONTEXT_FAILED || state == PA_CONTEXT_TERMINATED) {
            pa_context_unref(context);
            pa_mainloop_free(mainloop);
            return -3;
        }
        if (pa_mainloop_iterate(mainloop, 1, &ret) < 0) {
            pa_context_unref(context);
            pa_mainloop_free(mainloop);
            return -3;
        }
    }

    // source（入力デバイス）と sink（出力デバイス）を列挙する
    struct EnumerateContext enum_ctx = {NULL, 0, 0, 0};

    // source（入力デバイス）を列挙する
    pa_operation* op_source =
        pa_context_get_source_info_list(context, source_info_callback, &enum_ctx);
    if (!op_source) {
        pa_context_disconnect(context);
        pa_context_unref(context);
        pa_mainloop_free(mainloop);
        return -4;
    }

    // source 列挙が完了するまで待機する
    while (enum_ctx.done < 1) {
        pa_context_state_t state = pa_context_get_state(context);
        if (state == PA_CONTEXT_FAILED || state == PA_CONTEXT_TERMINATED) {
            break;
        }
        if (pa_mainloop_iterate(mainloop, 1, &ret) < 0) {
            break;
        }
    }
    pa_operation_unref(op_source);

    // sink（出力デバイス）を列挙する
    pa_operation* op_sink =
        pa_context_get_sink_info_list(context, sink_info_callback, &enum_ctx);
    if (!op_sink) {
        pa_context_disconnect(context);
        pa_context_unref(context);
        pa_mainloop_free(mainloop);
        // source の結果はそのまま返す
        *devices = enum_ctx.devices;
        *count = enum_ctx.count;
        return 0;
    }

    // sink 列挙が完了するまで待機する
    while (enum_ctx.done < 2) {
        pa_context_state_t state = pa_context_get_state(context);
        if (state == PA_CONTEXT_FAILED || state == PA_CONTEXT_TERMINATED) {
            break;
        }
        if (pa_mainloop_iterate(mainloop, 1, &ret) < 0) {
            break;
        }
    }
    pa_operation_unref(op_sink);

    pa_context_disconnect(context);
    pa_context_unref(context);
    pa_mainloop_free(mainloop);

    *devices = enum_ctx.devices;
    *count = enum_ctx.count;
    return 0;
}

void audio_pulse_free_devices(struct AudioDevice** devices, int count) {
    if (!devices) {
        return;
    }

    for (int i = 0; i < count; i++) {
        if (devices[i]) {
            free(devices[i]->name);
            free(devices[i]->unique_id);
            free(devices[i]);
        }
    }
    free(devices);
}

const char* audio_pulse_device_name(struct AudioDevice* device) {
    if (!device) {
        return NULL;
    }
    return device->name;
}

const char* audio_pulse_device_unique_id(struct AudioDevice* device) {
    if (!device) {
        return NULL;
    }
    return device->unique_id;
}

int audio_pulse_device_channels(struct AudioDevice* device) {
    if (!device) {
        return 0;
    }
    return device->channels;
}

int audio_pulse_device_sample_rate(struct AudioDevice* device) {
    if (!device) {
        return 0;
    }
    return device->sample_rate;
}

int audio_pulse_device_type(struct AudioDevice* device) {
    if (!device) {
        return AUDIO_DEVICE_TYPE_INPUT;
    }
    return device->device_type;
}

// PulseAudio 接続状態コールバック（セッション用）
static void session_state_callback(pa_context* c, void* userdata) {
    struct AudioSession* session = userdata;
    pa_context_state_t state = pa_context_get_state(c);

    switch (state) {
        case PA_CONTEXT_READY:
        case PA_CONTEXT_FAILED:
        case PA_CONTEXT_TERMINATED:
            pa_threaded_mainloop_signal(session->mainloop, 0);
            break;
        default:
            break;
    }
}

// 現在のモノトニック時刻をマイクロ秒で返す
static int64_t monotonic_time_us(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000 + ts.tv_nsec / 1000;
}

// パケット先頭フレームの録音時刻をマイクロ秒で求める。
//
// `oldest_timestamp_us` は、コールバックの先頭で求めたバッファ内で最も古い
// サンプルの録音時刻である。読み出し済みのバイト数 `read_bytes` だけ進んだ位置が、
// このパケットの先頭フレームの録音時刻になる。ただし pa_stream_get_latency() の値は
// 補間されるため、パケットごとの差に数 ms の揺らぎがある。そこで直前のパケットの
// 音声時間から予測した時刻を基本にし、予測から大きく外れた場合だけ latency の値で
// 補正する。タイミング情報を取得できない場合も、予測した時刻を使う。
//
// 戻り値は直前のパケットのタイムスタンプより必ず大きくなる。
static int64_t capture_timestamp_us(struct AudioSession* session,
                                    int has_oldest_timestamp,
                                    int64_t oldest_timestamp_us,
                                    size_t read_bytes, int frames) {
    int64_t timestamp_us;
    int frame_size = session->channels * 2;  // S16 の 1 サンプルは 2 バイト

    int64_t measured_timestamp_us = 0;
    int has_measured_timestamp = 0;
    if (has_oldest_timestamp && frame_size > 0 && session->sample_rate > 0) {
        int64_t read_frames = (int64_t)(read_bytes / frame_size);
        measured_timestamp_us = oldest_timestamp_us +
                                read_frames * 1000000 / session->sample_rate;
        has_measured_timestamp = 1;
    }

    if (has_measured_timestamp && !session->has_last_timestamp) {
        timestamp_us = measured_timestamp_us;
    } else if (session->has_last_timestamp) {
        // 直前のパケットの音声時間だけ進めた時刻を予測値にする
        timestamp_us = session->last_timestamp_us +
                       (int64_t)session->last_frames * 1000000 /
                           session->sample_rate;

        if (has_measured_timestamp) {
            int64_t difference_us = measured_timestamp_us - timestamp_us;
            if (difference_us > CAPTURE_TIMESTAMP_RESYNC_US ||
                difference_us < -CAPTURE_TIMESTAMP_RESYNC_US) {
                // オーバーフローなどで音声時間と録音時刻が大きくずれた場合は
                // latency の値で補正する
                timestamp_us = measured_timestamp_us;
            }
        }
    } else {
        timestamp_us = monotonic_time_us();
    }

    if (session->has_last_timestamp && timestamp_us <= session->last_timestamp_us) {
        timestamp_us = session->last_timestamp_us + 1;
    }

    session->last_timestamp_us = timestamp_us;
    session->has_last_timestamp = 1;
    session->last_frames = frames;

    return timestamp_us;
}

// ストリーム読み取りコールバック
static void stream_read_callback(pa_stream* s, size_t nbytes, void* userdata) {
    (void)nbytes;
    struct AudioSession* session = userdata;

    if (!atomic_load(&session->running)) {
        return;
    }

    const void* data;
    size_t length;

    // バッファ内で最も古いサンプルの録音時刻を求める。pa_stream_get_latency() は
    // 読み出し位置のサンプルが録音されてからアプリケーションに届くまでの時間を
    // 返すため、現在時刻から引くと最も古いサンプルの録音時刻になる。
    int64_t oldest_timestamp_us = 0;
    int has_oldest_timestamp = 0;
    pa_usec_t latency = 0;
    int negative = 0;
    if (pa_stream_get_latency(s, &latency, &negative) == 0 && !negative) {
        oldest_timestamp_us = monotonic_time_us() - (int64_t)latency;
        has_oldest_timestamp = 1;
    }

    // このコールバックでこれまでに読み出したバイト数
    size_t read_bytes = 0;

    while (pa_stream_peek(s, &data, &length) >= 0 && length > 0) {
        if (data && session->callback) {
            int bytes_per_sample = 2;  // S16 の 1 サンプルは 2 バイト
            int frame_size = bytes_per_sample * session->channels;
            int frames = (int)(length / frame_size);

            if (frames > 0) {
                int64_t timestamp_us = capture_timestamp_us(
                    session, has_oldest_timestamp, oldest_timestamp_us,
                    read_bytes, frames);

                session->callback(session->user_data, data, frames,
                                  session->channels, session->sample_rate,
                                  AUDIO_FORMAT_S16, timestamp_us);
            }
        }

        read_bytes += length;
        pa_stream_drop(s);
    }
}

// ストリーム状態コールバック
static void stream_state_callback(pa_stream* s, void* userdata) {
    struct AudioSession* session = userdata;
    pa_stream_state_t state = pa_stream_get_state(s);

    switch (state) {
        case PA_STREAM_READY:
        case PA_STREAM_FAILED:
        case PA_STREAM_TERMINATED:
            pa_threaded_mainloop_signal(session->mainloop, 0);
            break;
        default:
            break;
    }
}

struct AudioSession* audio_pulse_session_create(const char* device_id, int sample_rate,
                                          int channels) {
    struct AudioSession* session = calloc(1, sizeof(struct AudioSession));
    if (!session) {
        return NULL;
    }

    // デフォルト値の設定
    if (sample_rate <= 0) {
        sample_rate = 48000;
    }
    if (channels <= 0) {
        channels = 1;
    }

    session->sample_rate = sample_rate;
    session->channels = channels;
    session->device_id = device_id ? strdup(device_id) : NULL;
    atomic_init(&session->running, 0);

    // mainloop と context は start 時に作成する
    return session;
}

// セッションが保持する mainloop / context / stream を破棄する。
//
// mainloop が NULL でなければロックを取得して破棄する。呼び出し時は
// mainloop のロックを保持していないこと。
static void destroy_resources(pa_threaded_mainloop** mainloop,
                              pa_context** context,
                              pa_stream** stream) {
    if (!*mainloop) {
        return;
    }

    pa_threaded_mainloop_lock(*mainloop);

    if (*stream) {
        // PulseAudio のヘッダは用途終了時の disconnect を求めるため、
        // READY 以外でも disconnect してから unref する
        (void)pa_stream_disconnect(*stream);
        pa_stream_unref(*stream);
        *stream = NULL;
    }

    if (*context) {
        if (pa_context_get_state(*context) == PA_CONTEXT_READY) {
            pa_context_disconnect(*context);
        }
        pa_context_unref(*context);
        *context = NULL;
    }

    pa_threaded_mainloop_unlock(*mainloop);
    pa_threaded_mainloop_stop(*mainloop);
    pa_threaded_mainloop_free(*mainloop);
    *mainloop = NULL;
}

// mainloop と context を作成し、PulseAudio に接続して READY になるまで待機する。
//
// 成功時は mainloop のロックを保持したまま戻る。失敗時はロックを解放して
// mainloop / context / stream を破棄し、負の値を返す。
static int start_context(pa_threaded_mainloop** mainloop,
                         pa_context** context,
                         pa_stream** stream,
                         const char* context_name,
                         pa_context_notify_cb_t state_callback,
                         void* user_data) {
    *mainloop = pa_threaded_mainloop_new();
    if (!*mainloop) {
        return -2;
    }

    if (pa_threaded_mainloop_start(*mainloop) < 0) {
        pa_threaded_mainloop_free(*mainloop);
        *mainloop = NULL;
        return -2;
    }

    pa_threaded_mainloop_lock(*mainloop);

    *context = pa_context_new(
        pa_threaded_mainloop_get_api(*mainloop), context_name);
    if (!*context) {
        goto fail;
    }

    pa_context_set_state_callback(*context, state_callback, user_data);

    // PulseAudio に接続する
    if (pa_context_connect(*context, NULL, PA_CONTEXT_NOFLAGS, NULL) < 0) {
        goto fail;
    }

    // 接続が完了するまで待機する
    while (pa_context_get_state(*context) != PA_CONTEXT_READY) {
        pa_context_state_t state = pa_context_get_state(*context);
        if (state == PA_CONTEXT_FAILED || state == PA_CONTEXT_TERMINATED) {
            goto fail;
        }
        pa_threaded_mainloop_wait(*mainloop);
    }

    return 0;

fail:
    pa_threaded_mainloop_unlock(*mainloop);
    destroy_resources(mainloop, context, stream);
    return -3;
}

// 指定したサンプルレートとチャンネル数の S16 ストリームを作成する。
static pa_stream* create_stream(pa_context* context, const char* name,
                                int sample_rate, int channels) {
    pa_sample_spec sample_spec = {
        .format = PA_SAMPLE_S16LE,
        .rate = sample_rate,
        .channels = channels,
    };
    return pa_stream_new(context, name, &sample_spec, NULL);
}

// ストリームが READY になるまで待機する。
//
// mainloop のロックを保持した状態で呼び出す。READY 以外の失敗状態では
// 負の値を返す。
static int wait_stream_ready(pa_threaded_mainloop* mainloop,
                             pa_stream* stream) {
    while (pa_stream_get_state(stream) != PA_STREAM_READY) {
        pa_stream_state_t state = pa_stream_get_state(stream);
        if (state == PA_STREAM_FAILED || state == PA_STREAM_TERMINATED) {
            return -1;
        }
        pa_threaded_mainloop_wait(mainloop);
    }
    return 0;
}

void audio_pulse_session_destroy(struct AudioSession* session) {
    if (!session) {
        return;
    }

    // 実行中でも破棄できるよう、停止してからリソースを破棄する
    audio_pulse_session_stop(session);
    destroy_resources(&session->mainloop, &session->context, &session->stream);

    free(session->device_id);
    free(session);
}

int audio_pulse_session_start(struct AudioSession* session,
                        AudioFrameCallback callback,
                        void* user_data) {
    if (!session || !callback) {
        return -1;
    }

    if (atomic_load(&session->running)) {
        return 0;
    }

    // 前回のストリームのタイムスタンプを引き継がないよう、開始時に状態をリセットする
    session->has_last_timestamp = 0;

    session->callback = callback;
    session->user_data = user_data;

    int result = start_context(&session->mainloop, &session->context,
                               &session->stream, "shiguredo-audio-capture",
                               session_state_callback, session);
    if (result < 0) {
        return result;
    }

    result = -4;

    session->stream = create_stream(session->context, "audio-capture",
                                    session->sample_rate, session->channels);
    if (!session->stream) {
        goto fail;
    }

    pa_stream_set_state_callback(session->stream, stream_state_callback,
                                 session);
    pa_stream_set_read_callback(session->stream, stream_read_callback, session);

    result = -5;

    // ストリームを接続する
    // device_id が NULL の場合はデフォルトソースを使用する
    // パケット先頭フレームの録音時刻を pa_stream_get_latency() で取得するため、
    // タイミング情報の自動更新と補間を有効にする
    if (pa_stream_connect_record(session->stream, session->device_id, NULL,
                                 PA_STREAM_ADJUST_LATENCY |
                                     PA_STREAM_AUTO_TIMING_UPDATE |
                                     PA_STREAM_INTERPOLATE_TIMING) < 0) {
        goto fail;
    }

    if (wait_stream_ready(session->mainloop, session->stream) < 0) {
        goto fail;
    }

    atomic_store(&session->running, 1);
    pa_threaded_mainloop_unlock(session->mainloop);

    return 0;

fail:
    pa_threaded_mainloop_unlock(session->mainloop);
    destroy_resources(&session->mainloop, &session->context, &session->stream);
    return result;
}

int audio_pulse_session_stop(struct AudioSession* session) {
    if (!session) {
        return -1;
    }

    if (!atomic_load(&session->running)) {
        return 0;
    }

    atomic_store(&session->running, 0);
    destroy_resources(&session->mainloop, &session->context, &session->stream);
    return 0;
}

int audio_pulse_session_is_in_callback(struct AudioSession* session) {
    if (!session || !session->mainloop) {
        return 0;
    }
    return pa_threaded_mainloop_in_thread(session->mainloop) ? 1 : 0;
}

int audio_pulse_session_sample_rate(struct AudioSession* session) {
    if (!session) {
        return 0;
    }
    return session->sample_rate;
}

int audio_pulse_session_channels(struct AudioSession* session) {
    if (!session) {
        return 0;
    }
    return session->channels;
}

// --- 再生 (Playback) 実装 ---

struct PlaybackSession {
    pa_threaded_mainloop* mainloop;
    pa_context* context;
    pa_stream* stream;
    char* device_id;
    AudioPlaybackCallback callback;
    void* user_data;
    int sample_rate;
    int channels;
    atomic_int running;
};

// PulseAudio 接続状態コールバック（再生セッション用）
static void playback_state_callback(pa_context* c, void* userdata) {
    struct PlaybackSession* session = userdata;
    pa_context_state_t state = pa_context_get_state(c);

    switch (state) {
        case PA_CONTEXT_READY:
        case PA_CONTEXT_FAILED:
        case PA_CONTEXT_TERMINATED:
            pa_threaded_mainloop_signal(session->mainloop, 0);
            break;
        default:
            break;
    }
}

// 再生ストリーム書き込みコールバック
static void stream_write_callback(pa_stream* s, size_t nbytes, void* userdata) {
    struct PlaybackSession* session = userdata;

    if (nbytes == 0) {
        return;
    }

    void* buf;
    size_t len = nbytes;

    if (pa_stream_begin_write(s, &buf, &len) < 0 || !buf) {
        return;
    }

    if (!atomic_load(&session->running) || !session->callback) {
        // 無音を書き込む
        memset(buf, 0, len);
        pa_stream_write(s, buf, len, NULL, 0, PA_SEEK_RELATIVE);
        return;
    }

    int bytes_per_sample = 2;  // S16
    int frame_size = bytes_per_sample * session->channels;
    int frames = (int)(len / frame_size);

    if (frames <= 0) {
        memset(buf, 0, len);
        pa_stream_write(s, buf, len, NULL, 0, PA_SEEK_RELATIVE);
        return;
    }

    int written = session->callback(session->user_data, buf, frames,
                                     session->channels, session->sample_rate,
                                     AUDIO_FORMAT_S16);

    if (written <= 0) {
        memset(buf, 0, len);
    }
    // written > 0 の場合は Rust 側の共通変換関数がバッファ末尾をゼロ埋めしている

    pa_stream_write(s, buf, len, NULL, 0, PA_SEEK_RELATIVE);
}

// 再生ストリーム状態コールバック
static void playback_stream_state_callback(pa_stream* s, void* userdata) {
    struct PlaybackSession* session = userdata;
    pa_stream_state_t state = pa_stream_get_state(s);

    switch (state) {
        case PA_STREAM_READY:
        case PA_STREAM_FAILED:
        case PA_STREAM_TERMINATED:
            pa_threaded_mainloop_signal(session->mainloop, 0);
            break;
        default:
            break;
    }
}

struct PlaybackSession* audio_pulse_playback_session_create(const char* device_id,
                                                            int sample_rate,
                                                            int channels) {
    struct PlaybackSession* session = calloc(1, sizeof(struct PlaybackSession));
    if (!session) {
        return NULL;
    }

    // デフォルト値の設定
    if (sample_rate <= 0) {
        sample_rate = 48000;
    }
    if (channels <= 0) {
        channels = 2;
    }

    session->sample_rate = sample_rate;
    session->channels = channels;
    session->device_id = device_id ? strdup(device_id) : NULL;
    atomic_init(&session->running, 0);

    // mainloop と context は start 時に作成する
    return session;
}

void audio_pulse_playback_session_destroy(struct PlaybackSession* session) {
    if (!session) {
        return;
    }

    // 実行中でも破棄できるよう、停止してからリソースを破棄する
    audio_pulse_playback_session_stop(session);
    destroy_resources(&session->mainloop, &session->context, &session->stream);

    free(session->device_id);
    free(session);
}

int audio_pulse_playback_session_start(struct PlaybackSession* session,
                                       AudioPlaybackCallback callback,
                                       void* user_data) {
    if (!session || !callback) {
        return -1;
    }

    if (atomic_load(&session->running)) {
        return 0;
    }

    session->callback = callback;
    session->user_data = user_data;

    int result = start_context(&session->mainloop, &session->context,
                               &session->stream, "shiguredo-audio-playback",
                               playback_state_callback, session);
    if (result < 0) {
        return result;
    }

    result = -4;

    session->stream = create_stream(session->context, "audio-playback",
                                    session->sample_rate, session->channels);
    if (!session->stream) {
        goto fail;
    }

    pa_stream_set_state_callback(session->stream,
                                 playback_stream_state_callback, session);
    pa_stream_set_write_callback(session->stream, stream_write_callback,
                                 session);

    result = -5;

    // 再生ストリームを接続する
    // device_id が NULL の場合はデフォルトシンクを使用する
    if (pa_stream_connect_playback(session->stream, session->device_id, NULL,
                                   PA_STREAM_ADJUST_LATENCY, NULL, NULL) < 0) {
        goto fail;
    }

    if (wait_stream_ready(session->mainloop, session->stream) < 0) {
        goto fail;
    }

    atomic_store(&session->running, 1);
    pa_threaded_mainloop_unlock(session->mainloop);

    return 0;

fail:
    pa_threaded_mainloop_unlock(session->mainloop);
    destroy_resources(&session->mainloop, &session->context, &session->stream);
    return result;
}

int audio_pulse_playback_session_stop(struct PlaybackSession* session) {
    if (!session) {
        return -1;
    }

    if (!atomic_load(&session->running)) {
        return 0;
    }

    atomic_store(&session->running, 0);
    destroy_resources(&session->mainloop, &session->context, &session->stream);
    return 0;
}

int audio_pulse_playback_session_is_in_callback(struct PlaybackSession* session) {
    if (!session || !session->mainloop) {
        return 0;
    }
    return pa_threaded_mainloop_in_thread(session->mainloop) ? 1 : 0;
}

int audio_pulse_playback_session_sample_rate(struct PlaybackSession* session) {
    if (!session) {
        return 0;
    }
    return session->sample_rate;
}

int audio_pulse_playback_session_channels(struct PlaybackSession* session) {
    if (!session) {
        return 0;
    }
    return session->channels;
}
