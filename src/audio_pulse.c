// PulseAudio を使った Linux 用オーディオキャプチャ実装

#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <pulse/pulseaudio.h>

#include "audio_c.h"

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
    // PulseAudio の source name を一意識別子として使用する
    device->unique_id = strdup(info->name);
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
    device->unique_id = strdup(info->name);
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
int audio_enumerate_devices(struct AudioDevice*** devices, int* count) {
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

void audio_free_devices(struct AudioDevice** devices, int count) {
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

const char* audio_device_name(struct AudioDevice* device) {
    if (!device) {
        return NULL;
    }
    return device->name;
}

const char* audio_device_unique_id(struct AudioDevice* device) {
    if (!device) {
        return NULL;
    }
    return device->unique_id;
}

int audio_device_channels(struct AudioDevice* device) {
    if (!device) {
        return 0;
    }
    return device->channels;
}

int audio_device_sample_rate(struct AudioDevice* device) {
    if (!device) {
        return 0;
    }
    return device->sample_rate;
}

int audio_device_type(struct AudioDevice* device) {
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

// ストリーム読み取りコールバック
static void stream_read_callback(pa_stream* s, size_t nbytes, void* userdata) {
    (void)nbytes;
    struct AudioSession* session = userdata;

    if (!atomic_load(&session->running)) {
        return;
    }

    const void* data;
    size_t length;

    while (pa_stream_peek(s, &data, &length) >= 0 && length > 0) {
        if (data && session->callback) {
            int bytes_per_sample = 2;  // S16
            int frame_size = bytes_per_sample * session->channels;
            int frames = (int)(length / frame_size);

            if (frames > 0) {
                // タイムスタンプを取得（マイクロ秒）
                struct timespec ts;
                clock_gettime(CLOCK_MONOTONIC, &ts);
                int64_t timestamp_us =
                    (int64_t)ts.tv_sec * 1000000 + ts.tv_nsec / 1000;

                session->callback(session->user_data, data, frames,
                                  session->channels, session->sample_rate,
                                  AUDIO_FORMAT_S16, timestamp_us);
            }
        }

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

struct AudioSession* audio_session_create(const char* device_id, int sample_rate,
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

    // threaded mainloop を作成する
    session->mainloop = pa_threaded_mainloop_new();
    if (!session->mainloop) {
        free(session->device_id);
        free(session);
        return NULL;
    }

    pa_mainloop_api* api = pa_threaded_mainloop_get_api(session->mainloop);
    session->context = pa_context_new(api, "shiguredo-audio-capture");
    if (!session->context) {
        pa_threaded_mainloop_free(session->mainloop);
        free(session->device_id);
        free(session);
        return NULL;
    }

    pa_context_set_state_callback(session->context, session_state_callback,
                                  session);

    return session;
}

void audio_session_destroy(struct AudioSession* session) {
    if (!session) {
        return;
    }

    if (atomic_load(&session->running)) {
        audio_session_stop(session);
    }

    if (session->stream) {
        pa_stream_unref(session->stream);
    }

    if (session->context) {
        pa_context_disconnect(session->context);
        pa_context_unref(session->context);
    }

    if (session->mainloop) {
        pa_threaded_mainloop_free(session->mainloop);
    }

    free(session->device_id);
    free(session);
}

int audio_session_start(struct AudioSession* session,
                        AudioFrameCallback callback,
                        void* user_data) {
    if (!session || !callback) {
        return -1;
    }

    if (atomic_load(&session->running)) {
        return 0;
    }

    session->callback = callback;
    session->user_data = user_data;

    // threaded mainloop を開始する
    if (pa_threaded_mainloop_start(session->mainloop) < 0) {
        return -2;
    }

    pa_threaded_mainloop_lock(session->mainloop);

    // PulseAudio に接続する
    if (pa_context_connect(session->context, NULL, PA_CONTEXT_NOFLAGS, NULL) <
        0) {
        pa_threaded_mainloop_unlock(session->mainloop);
        pa_threaded_mainloop_stop(session->mainloop);
        return -3;
    }

    // 接続が完了するまで待機する
    while (pa_context_get_state(session->context) != PA_CONTEXT_READY) {
        pa_context_state_t state = pa_context_get_state(session->context);
        if (state == PA_CONTEXT_FAILED || state == PA_CONTEXT_TERMINATED) {
            pa_threaded_mainloop_unlock(session->mainloop);
            pa_threaded_mainloop_stop(session->mainloop);
            return -3;
        }
        pa_threaded_mainloop_wait(session->mainloop);
    }

    // ストリームを作成する
    pa_sample_spec sample_spec = {
        .format = PA_SAMPLE_S16LE,
        .rate = session->sample_rate,
        .channels = session->channels,
    };

    session->stream =
        pa_stream_new(session->context, "audio-capture", &sample_spec, NULL);
    if (!session->stream) {
        pa_threaded_mainloop_unlock(session->mainloop);
        pa_threaded_mainloop_stop(session->mainloop);
        return -4;
    }

    pa_stream_set_state_callback(session->stream, stream_state_callback,
                                 session);
    pa_stream_set_read_callback(session->stream, stream_read_callback, session);

    // ストリームを接続する
    // device_id が NULL の場合はデフォルトソースを使用する
    if (pa_stream_connect_record(session->stream, session->device_id, NULL,
                                 PA_STREAM_ADJUST_LATENCY) < 0) {
        pa_stream_unref(session->stream);
        session->stream = NULL;
        pa_threaded_mainloop_unlock(session->mainloop);
        pa_threaded_mainloop_stop(session->mainloop);
        return -5;
    }

    // ストリームが準備完了するまで待機する
    while (pa_stream_get_state(session->stream) != PA_STREAM_READY) {
        pa_stream_state_t state = pa_stream_get_state(session->stream);
        if (state == PA_STREAM_FAILED || state == PA_STREAM_TERMINATED) {
            pa_stream_unref(session->stream);
            session->stream = NULL;
            pa_threaded_mainloop_unlock(session->mainloop);
            pa_threaded_mainloop_stop(session->mainloop);
            return -5;
        }
        pa_threaded_mainloop_wait(session->mainloop);
    }

    atomic_store(&session->running, 1);
    pa_threaded_mainloop_unlock(session->mainloop);

    return 0;
}

void audio_session_stop(struct AudioSession* session) {
    if (!session || !atomic_load(&session->running)) {
        return;
    }

    atomic_store(&session->running, 0);

    pa_threaded_mainloop_lock(session->mainloop);

    if (session->stream) {
        pa_stream_disconnect(session->stream);
        pa_stream_unref(session->stream);
        session->stream = NULL;
    }

    pa_threaded_mainloop_unlock(session->mainloop);

    pa_threaded_mainloop_stop(session->mainloop);
}

int audio_session_sample_rate(struct AudioSession* session) {
    if (!session) {
        return 0;
    }
    return session->sample_rate;
}

int audio_session_channels(struct AudioSession* session) {
    if (!session) {
        return 0;
    }
    return session->channels;
}
