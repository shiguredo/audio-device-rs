// PipeWire を使った Linux 用オーディオキャプチャ・再生実装

#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <pipewire/pipewire.h>
#include <spa/param/audio/format-utils.h>

#include "audio_pipewire.h"

// ストリーム状態の待機タイムアウト
#define STREAM_STATE_TIMEOUT_SEC 10

// 状態変化のシグナルを取りこぼしても状態を再確認するための待機間隔
#define STREAM_STATE_POLL_INTERVAL_SEC 1

// 待機ループのタイムアウト (ナノ秒)
#define STREAM_STATE_TIMEOUT_NS \
    ((int64_t)STREAM_STATE_TIMEOUT_SEC * SPA_NSEC_PER_SEC)

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
    struct pw_thread_loop* thread_loop;
    struct pw_context* context;
    struct pw_core* core;
    struct pw_stream* stream;
    struct spa_hook core_listener;
    struct spa_hook stream_listener;
    char* device_id;
    AudioFrameCallback callback;
    void* user_data;
    int sample_rate;
    int channels;
    atomic_int running;
    // core の error イベントで受けたエラーコード
    atomic_int core_error;
    // 直前にコールバックへ渡したバッファのタイムスタンプとフレーム数
    int64_t last_timestamp_us;
    int last_frames;
    int has_last_timestamp;
};

// デバイス列挙用のコンテキスト
struct EnumerateContext {
    struct pw_main_loop* loop;
    struct pw_context* context;
    struct pw_core* core;
    struct pw_registry* registry;
    struct spa_hook registry_listener;
    struct spa_hook core_listener;
    struct AudioDevice** devices;
    int count;
    int capacity;
    int pending_sync;
};

// デバイス列挙: registry global イベント
static void enum_registry_global(void* data, uint32_t id,
                                 uint32_t permissions, const char* type,
                                 uint32_t version,
                                 const struct spa_dict* props) {
    (void)id;
    (void)permissions;
    (void)version;

    struct EnumerateContext* ctx = data;

    if (strcmp(type, PW_TYPE_INTERFACE_Node) != 0) {
        return;
    }

    if (!props) {
        return;
    }

    // Audio/Source と Audio/Sink を対象にする (monitor を除外する)
    const char* media_class = spa_dict_lookup(props, PW_KEY_MEDIA_CLASS);
    if (!media_class) {
        return;
    }

    int device_type;
    if (strcmp(media_class, "Audio/Source") == 0) {
        device_type = AUDIO_DEVICE_TYPE_INPUT;
    } else if (strcmp(media_class, "Audio/Sink") == 0) {
        device_type = AUDIO_DEVICE_TYPE_OUTPUT;
    } else {
        return;
    }

    const char* node_name = spa_dict_lookup(props, PW_KEY_NODE_NAME);
    const char* node_description = spa_dict_lookup(props, PW_KEY_NODE_DESCRIPTION);
    if (!node_name) {
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

    device->name = strdup(node_description ? node_description : node_name);
    if (!device->name) {
        free(device);
        return;
    }

    device->unique_id = strdup(node_name);
    if (!device->unique_id) {
        free(device->name);
        free(device);
        return;
    }

    // チャンネル数とサンプルレートはプロパティから取得を試みる
    const char* channels_str =
        spa_dict_lookup(props, PW_KEY_AUDIO_CHANNELS);
    const char* rate_str = spa_dict_lookup(props, PW_KEY_AUDIO_RATE);

    device->channels = channels_str ? atoi(channels_str) : 1;
    if (device->channels <= 0) {
        device->channels = 1;
    }

    device->sample_rate = rate_str ? atoi(rate_str) : 48000;
    if (device->sample_rate <= 0) {
        device->sample_rate = 48000;
    }

    device->device_type = device_type;

    ctx->devices[ctx->count] = device;
    ctx->count++;
}

static const struct pw_registry_events enum_registry_events = {
    PW_VERSION_REGISTRY_EVENTS,
    .global = enum_registry_global,
};

// デバイス列挙: core done イベント (sync 完了)
static void enum_core_done(void* data, uint32_t id, int seq) {
    struct EnumerateContext* ctx = data;
    (void)id;

    if (seq == ctx->pending_sync) {
        pw_main_loop_quit(ctx->loop);
    }
}

static const struct pw_core_events enum_core_events = {
    PW_VERSION_CORE_EVENTS,
    .done = enum_core_done,
};

// デバイス列挙
int audio_pipewire_enumerate_devices(struct AudioDevice*** devices, int* count) {
    if (!devices || !count) {
        return -1;
    }

    *devices = NULL;
    *count = 0;

    pw_init(NULL, NULL);

    struct EnumerateContext ctx = {0};

    ctx.loop = pw_main_loop_new(NULL);
    if (!ctx.loop) {
        return -2;
    }

    ctx.context =
        pw_context_new(pw_main_loop_get_loop(ctx.loop), NULL, 0);
    if (!ctx.context) {
        pw_main_loop_destroy(ctx.loop);
        return -2;
    }

    ctx.core = pw_context_connect(ctx.context, NULL, 0);
    if (!ctx.core) {
        pw_context_destroy(ctx.context);
        pw_main_loop_destroy(ctx.loop);
        return -3;
    }

    // core の done イベントを監視する
    spa_zero(ctx.core_listener);
    pw_core_add_listener(ctx.core, &ctx.core_listener, &enum_core_events,
                         &ctx);

    ctx.registry = pw_core_get_registry(ctx.core, PW_VERSION_REGISTRY, 0);
    if (!ctx.registry) {
        pw_core_disconnect(ctx.core);
        pw_context_destroy(ctx.context);
        pw_main_loop_destroy(ctx.loop);
        return -4;
    }

    spa_zero(ctx.registry_listener);
    pw_registry_add_listener(ctx.registry, &ctx.registry_listener,
                             &enum_registry_events, &ctx);

    // sync を送って全 global の列挙完了を待つ
    ctx.pending_sync = pw_core_sync(ctx.core, PW_ID_CORE, 0);

    pw_main_loop_run(ctx.loop);

    // クリーンアップ
    spa_hook_remove(&ctx.registry_listener);
    pw_proxy_destroy((struct pw_proxy*)ctx.registry);
    spa_hook_remove(&ctx.core_listener);
    pw_core_disconnect(ctx.core);
    pw_context_destroy(ctx.context);
    pw_main_loop_destroy(ctx.loop);

    *devices = ctx.devices;
    *count = ctx.count;
    return 0;
}

void audio_pipewire_free_devices(struct AudioDevice** devices, int count) {
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

const char* audio_pipewire_device_name(struct AudioDevice* device) {
    if (!device) {
        return NULL;
    }
    return device->name;
}

const char* audio_pipewire_device_unique_id(struct AudioDevice* device) {
    if (!device) {
        return NULL;
    }
    return device->unique_id;
}

int audio_pipewire_device_channels(struct AudioDevice* device) {
    if (!device) {
        return 0;
    }
    return device->channels;
}

int audio_pipewire_device_sample_rate(struct AudioDevice* device) {
    if (!device) {
        return 0;
    }
    return device->sample_rate;
}

int audio_pipewire_device_type(struct AudioDevice* device) {
    if (!device) {
        return AUDIO_DEVICE_TYPE_INPUT;
    }
    return device->device_type;
}

// 現在のストリーム時刻をマイクロ秒で返す
static int64_t stream_time_us(struct AudioSession* session) {
#if PW_CHECK_VERSION(1, 0, 4)
    return (int64_t)(pw_stream_get_nsec(session->stream) / 1000);
#else
    // pw_stream_get_nsec() を利用できないビルドではモノトニック時刻を使う
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000 + ts.tv_nsec / 1000;
#endif
}

// キャプチャバッファの先頭フレームの録音時刻をマイクロ秒で求める。
//
// pw_buffer.time は PipeWire がバッファをストリームへ渡したときのサイクル時刻
// (ナノ秒) であり、pw_stream_get_nsec() と同じクロックで比較できる。time が 0 の
// 場合は、直前のバッファの音声時間だけ進めた時刻か、ストリーム時刻へフォールバック
// する。
//
// 戻り値は直前のバッファのタイムスタンプより必ず大きくなる。
static int64_t capture_timestamp_us(struct AudioSession* session,
                                    uint64_t buffer_time_ns, int frames) {
    int64_t timestamp_us;
    if (buffer_time_ns > 0) {
        timestamp_us = (int64_t)(buffer_time_ns / 1000);
    } else if (session->has_last_timestamp && session->sample_rate > 0) {
        // サイクル時刻を取得できない場合は、直前のバッファの音声時間だけ進める
        timestamp_us = session->last_timestamp_us +
                       (int64_t)session->last_frames * 1000000 /
                           session->sample_rate;
    } else {
        timestamp_us = stream_time_us(session);
    }

    if (session->has_last_timestamp && timestamp_us <= session->last_timestamp_us) {
        timestamp_us = session->last_timestamp_us + 1;
    }

    session->last_timestamp_us = timestamp_us;
    session->has_last_timestamp = 1;
    session->last_frames = frames;

    return timestamp_us;
}

// ストリームの process コールバック
static void on_process(void* userdata) {
    struct AudioSession* session = userdata;

    if (!atomic_load(&session->running)) {
        return;
    }

    struct pw_buffer* buf = pw_stream_dequeue_buffer(session->stream);
    if (!buf) {
        return;
    }

    struct spa_buffer* spa_buf = buf->buffer;
    if (spa_buf->n_datas < 1 || !spa_buf->datas[0].data ||
        !spa_buf->datas[0].chunk) {
        pw_stream_queue_buffer(session->stream, buf);
        return;
    }

    const void* data = spa_buf->datas[0].data;
    // chunk->size は PipeWire サーバが書き込む値であり、data が指す領域の大きさ
    // (maxsize) を超える可能性がある。超えた値を読み取るとバッファ外を参照する
    // ため、maxsize にクランプする
    uint32_t maxsize = spa_buf->datas[0].maxsize;
    uint32_t size = spa_buf->datas[0].chunk->size;
    if (size > maxsize) {
        size = maxsize;
    }

    if (size > 0 && session->callback) {
        int bytes_per_sample = 2;  // S16 の 1 サンプルは 2 バイト
        int frame_size = bytes_per_sample * session->channels;
        int frames = (int)(size / frame_size);

        if (frames > 0) {
#if PW_CHECK_VERSION(1, 0, 5)
            int64_t timestamp_us =
                capture_timestamp_us(session, buf->time, frames);
#else
            // pw_buffer.time を利用できないビルドではストリーム時刻を使う
            int64_t timestamp_us = capture_timestamp_us(session, 0, frames);
#endif

            session->callback(session->user_data, data, frames,
                              session->channels, session->sample_rate,
                              AUDIO_FORMAT_S16, timestamp_us);
        }
    }

    pw_stream_queue_buffer(session->stream, buf);
}

// ストリーム状態変更コールバック
static void on_stream_state_changed(void* userdata,
                                    enum pw_stream_state old,
                                    enum pw_stream_state state,
                                    const char* error) {
    (void)old;
    (void)error;

    struct AudioSession* session = userdata;

    switch (state) {
        case PW_STREAM_STATE_STREAMING:
        case PW_STREAM_STATE_PAUSED:
        case PW_STREAM_STATE_ERROR:
        case PW_STREAM_STATE_UNCONNECTED:
            pw_thread_loop_signal(session->thread_loop, false);
            break;
        default:
            break;
    }
}

static const struct pw_stream_events stream_events = {
    PW_VERSION_STREAM_EVENTS,
    .process = on_process,
    .state_changed = on_stream_state_changed,
};

// core の error イベント
static void session_core_error(void* data, uint32_t id, int seq, int res,
                               const char* message) {
    (void)id;
    (void)seq;
    (void)message;
    struct AudioSession* session = data;
    // C API はエラーコードのみを返すため、メッセージは保持しない
    atomic_store(&session->core_error, res);
    pw_thread_loop_signal(session->thread_loop, false);
}

static const struct pw_core_events session_core_events = {
    PW_VERSION_CORE_EVENTS,
    .error = session_core_error,
};

struct AudioSession* audio_pipewire_session_create(const char* device_id, int sample_rate,
                                          int channels) {
    struct AudioSession* session = calloc(1, sizeof(struct AudioSession));
    if (!session) {
        return NULL;
    }

    pw_init(NULL, NULL);

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
    atomic_init(&session->core_error, 0);

    // thread loop を作成する
    session->thread_loop = pw_thread_loop_new("shiguredo-audio", NULL);
    if (!session->thread_loop) {
        free(session->device_id);
        free(session);
        return NULL;
    }

    session->context = pw_context_new(
        pw_thread_loop_get_loop(session->thread_loop), NULL, 0);
    if (!session->context) {
        pw_thread_loop_destroy(session->thread_loop);
        free(session->device_id);
        free(session);
        return NULL;
    }

    return session;
}

void audio_pipewire_session_destroy(struct AudioSession* session) {
    if (!session) {
        return;
    }

    // 実行中でも破棄できるよう、停止してからリソースを破棄する
    audio_pipewire_session_stop(session);

    pw_context_destroy(session->context);
    pw_thread_loop_destroy(session->thread_loop);

    free(session->device_id);
    free(session);
}

// セッションの stream と core を破棄して thread loop を停止する。
//
// thread loop のロックを保持した状態で呼び出す。stream を切断してからロックを
// 解放し、thread loop の停止で保留中の process invoke を処理した後に stream を
// 破棄する。停止前に stream を破棄すると process invoke が破棄済みの stream を
// 参照するため、この順序でなければならない。その後 core_listener を remove して
// core を切断し、stream と core には NULL を代入する。
static void destroy_session_resources(struct pw_thread_loop* thread_loop,
                                      struct pw_stream** stream,
                                      struct spa_hook* core_listener,
                                      struct pw_core** core) {
    if (*stream) {
        pw_stream_disconnect(*stream);
    }
    pw_thread_loop_unlock(thread_loop);
    pw_thread_loop_stop(thread_loop);
    if (*stream) {
        pw_stream_destroy(*stream);
        *stream = NULL;
    }
    if (*core) {
        spa_hook_remove(core_listener);
        pw_core_disconnect(*core);
        *core = NULL;
    }
}

// ストリームが STREAMING または PAUSED になるまで待機する。
//
// thread loop のロックを保持した状態で呼び出す。成功時は 0、失敗時は
// AudioPipeWireResult の負の値を返す。失敗時の破棄は呼び出し側が行う。
//
// pw_thread_loop_timed_wait は CLOCK_REALTIME の cond を使うが、経過時間は
// CLOCK_MONOTONIC で判定する。システム時刻の後退やサスペンドを含む環境では
// STREAM_STATE_TIMEOUT_SEC が厳密な上限にならない。
static int wait_for_stream_ready(struct pw_thread_loop* thread_loop,
                                 struct pw_stream* stream,
                                 atomic_int* core_error) {
    struct timespec start_time;
    if (clock_gettime(CLOCK_MONOTONIC, &start_time) != 0) {
        return AUDIO_PIPEWIRE_ERROR_CLOCK;
    }

    while (1) {
        if (atomic_load(core_error) < 0) {
            return AUDIO_PIPEWIRE_ERROR_CORE;
        }

        enum pw_stream_state state = pw_stream_get_state(stream, NULL);
        if (state == PW_STREAM_STATE_STREAMING) {
            return AUDIO_PIPEWIRE_RESULT_OK;
        }
        // PAUSED でもストリームは作成済みなので、activate して開始成功として扱う
        if (state == PW_STREAM_STATE_PAUSED) {
            if (pw_stream_set_active(stream, true) < 0) {
                return AUDIO_PIPEWIRE_ERROR_SET_ACTIVE;
            }
            return AUDIO_PIPEWIRE_RESULT_OK;
        }
        if (state == PW_STREAM_STATE_ERROR ||
            state == PW_STREAM_STATE_UNCONNECTED) {
            return AUDIO_PIPEWIRE_ERROR_STREAM_STATE;
        }

        struct timespec now;
        if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
            return AUDIO_PIPEWIRE_ERROR_CLOCK;
        }
        int64_t elapsed_ns =
            (int64_t)(now.tv_sec - start_time.tv_sec) * SPA_NSEC_PER_SEC +
            (now.tv_nsec - start_time.tv_nsec);
        if (elapsed_ns >= STREAM_STATE_TIMEOUT_NS) {
            return AUDIO_PIPEWIRE_ERROR_TIMEOUT;
        }

        // 状態変化のシグナルを取りこぼしても状態と経過時間を確認できるよう、
        // 最大 STREAM_STATE_POLL_INTERVAL_SEC 秒待機する。戻り値は 0 = シグナル、
        // 正 = タイムアウト、負 = エラー
        int wait_result = pw_thread_loop_timed_wait(
            thread_loop, STREAM_STATE_POLL_INTERVAL_SEC);
        if (wait_result < 0) {
            return AUDIO_PIPEWIRE_ERROR_WAIT;
        }
    }
}

int audio_pipewire_session_start(struct AudioSession* session,
                        AudioFrameCallback callback,
                        void* user_data) {
    if (!session || !callback) {
        return AUDIO_PIPEWIRE_ERROR_INVALID_ARGUMENT;
    }

    if (atomic_load(&session->running)) {
        return AUDIO_PIPEWIRE_RESULT_OK;
    }

    // 前回のストリームのタイムスタンプを引き継がないよう、開始時に状態をリセットする
    session->has_last_timestamp = 0;

    session->callback = callback;
    session->user_data = user_data;

    // thread loop を開始する
    if (pw_thread_loop_start(session->thread_loop) < 0) {
        return AUDIO_PIPEWIRE_ERROR_THREAD_LOOP_START;
    }

    pw_thread_loop_lock(session->thread_loop);

    int result = AUDIO_PIPEWIRE_RESULT_OK;
    struct pw_properties* props = NULL;

    atomic_store(&session->core_error, 0);

    // PipeWire に接続する
    session->core = pw_context_connect(session->context, NULL, 0);
    if (!session->core) {
        result = AUDIO_PIPEWIRE_ERROR_CONTEXT_CONNECT;
        goto fail;
    }

    spa_zero(session->core_listener);
    pw_core_add_listener(session->core, &session->core_listener,
                         &session_core_events, session);

    // ストリームのプロパティを設定する
    props = pw_properties_new(PW_KEY_MEDIA_TYPE, "Audio",
                              PW_KEY_MEDIA_CATEGORY, "Capture",
                              PW_KEY_MEDIA_ROLE, "Communication", NULL);
    if (!props) {
        result = AUDIO_PIPEWIRE_ERROR_PROPERTIES;
        goto fail;
    }

    if (session->device_id) {
        pw_properties_set(props, PW_KEY_TARGET_OBJECT,
                          session->device_id);
    }

    // ストリームを作成する
    // props の所有権は pw_stream_new に移るため、呼び出し後に解放してはいけない
    session->stream =
        pw_stream_new(session->core, "audio-capture", props);
    if (!session->stream) {
        result = AUDIO_PIPEWIRE_ERROR_STREAM_CREATE;
        goto fail;
    }

    spa_zero(session->stream_listener);
    pw_stream_add_listener(session->stream, &session->stream_listener,
                           &stream_events, session);

    // オーディオフォーマットを設定する
    uint8_t params_buffer[1024];
    struct spa_pod_builder builder =
        SPA_POD_BUILDER_INIT(params_buffer, sizeof(params_buffer));

    struct spa_audio_info_raw audio_info = SPA_AUDIO_INFO_RAW_INIT(
        .format = SPA_AUDIO_FORMAT_S16_LE,
        .rate = session->sample_rate,
        .channels = session->channels);

    const struct spa_pod* params[1];
    params[0] = spa_format_audio_raw_build(&builder, SPA_PARAM_EnumFormat,
                                           &audio_info);

    // ストリームを接続する (PW_STREAM_FLAG_RT_PROCESS は使わない)
    if (pw_stream_connect(session->stream, PW_DIRECTION_INPUT, PW_ID_ANY,
                          PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS,
                          params, 1) < 0) {
        result = AUDIO_PIPEWIRE_ERROR_STREAM_CONNECT;
        goto fail;
    }

    result = wait_for_stream_ready(session->thread_loop, session->stream,
                                   &session->core_error);
    if (result < 0) {
        goto fail;
    }

    atomic_store(&session->running, 1);
    pw_thread_loop_unlock(session->thread_loop);

    return AUDIO_PIPEWIRE_RESULT_OK;

fail:
    destroy_session_resources(session->thread_loop, &session->stream,
                              &session->core_listener, &session->core);
    return result;
}

int audio_pipewire_session_stop(struct AudioSession* session) {
    if (!session) {
        return -1;
    }

    if (!atomic_load(&session->running) && !session->stream && !session->core) {
        return 0;
    }

    atomic_store(&session->running, 0);

    pw_thread_loop_lock(session->thread_loop);
    destroy_session_resources(session->thread_loop, &session->stream,
                              &session->core_listener, &session->core);
    return 0;
}

int audio_pipewire_session_is_in_callback(struct AudioSession* session) {
    if (!session) {
        return 0;
    }
    return pw_thread_loop_in_thread(session->thread_loop) ? 1 : 0;
}

int audio_pipewire_session_sample_rate(struct AudioSession* session) {
    if (!session) {
        return 0;
    }
    return session->sample_rate;
}

int audio_pipewire_session_channels(struct AudioSession* session) {
    if (!session) {
        return 0;
    }
    return session->channels;
}

// --- 再生 (Playback) 実装 ---

struct PlaybackSession {
    struct pw_thread_loop* thread_loop;
    struct pw_context* context;
    struct pw_core* core;
    struct pw_stream* stream;
    struct spa_hook core_listener;
    struct spa_hook stream_listener;
    char* device_id;
    AudioPlaybackCallback callback;
    void* user_data;
    int sample_rate;
    int channels;
    atomic_int running;
    // core の error イベントで受けたエラーコード
    atomic_int core_error;
};

// 再生ストリームの process コールバック
static void playback_on_process(void* userdata) {
    struct PlaybackSession* session = userdata;

    struct pw_buffer* buf = pw_stream_dequeue_buffer(session->stream);
    if (!buf) {
        return;
    }

    if (!atomic_load(&session->running)) {
        pw_stream_queue_buffer(session->stream, buf);
        return;
    }

    struct spa_buffer* spa_buf = buf->buffer;
    if (spa_buf->n_datas < 1 || !spa_buf->datas[0].data || !spa_buf->datas[0].chunk) {
        pw_stream_queue_buffer(session->stream, buf);
        return;
    }

    void* data = spa_buf->datas[0].data;
    uint32_t max_size = spa_buf->datas[0].maxsize;

    int bytes_per_sample = 2;  // S16
    int frame_size = bytes_per_sample * session->channels;
    uint32_t max_frames = max_size / frame_size;

    // PipeWire が要求したサンプル数を優先し、要求がない場合だけ
    // バッファ容量を使う
    uint32_t frames = max_frames;
#if PW_CHECK_VERSION(0, 3, 49)
    if (buf->requested > 0 && buf->requested <= max_frames) {
        frames = (uint32_t)buf->requested;
    }
#endif

    if (frames > 0 && session->callback) {
        int written = session->callback(session->user_data, data, frames,
                                         session->channels, session->sample_rate,
                                         AUDIO_FORMAT_S16);

        // コールバックが要求フレーム数を超える値を返した場合はクランプする
        if (written > (int)frames) {
            written = (int)frames;
        }

        if (written <= 0) {
            memset(data, 0, frames * frame_size);
        }
        // written > 0 の場合は Rust 側の共通変換関数がバッファ末尾をゼロ埋めしている
        spa_buf->datas[0].chunk->size = frames * frame_size;
    } else {
        memset(data, 0, frames * frame_size);
        spa_buf->datas[0].chunk->size = frames * frame_size;
    }

    spa_buf->datas[0].chunk->offset = 0;
    spa_buf->datas[0].chunk->stride = frame_size;
    // キューに追加するサンプル数を記録する
    buf->size = frames;

    pw_stream_queue_buffer(session->stream, buf);
}

// 再生ストリーム状態変更コールバック
static void playback_on_stream_state_changed(void* userdata,
                                              enum pw_stream_state old,
                                              enum pw_stream_state state,
                                              const char* error) {
    (void)old;
    (void)error;

    struct PlaybackSession* session = userdata;

    switch (state) {
        case PW_STREAM_STATE_STREAMING:
        case PW_STREAM_STATE_PAUSED:
        case PW_STREAM_STATE_ERROR:
        case PW_STREAM_STATE_UNCONNECTED:
            pw_thread_loop_signal(session->thread_loop, false);
            break;
        default:
            break;
    }
}

static const struct pw_stream_events playback_stream_events = {
    PW_VERSION_STREAM_EVENTS,
    .process = playback_on_process,
    .state_changed = playback_on_stream_state_changed,
};

// 再生用 core error イベント
static void playback_core_error(void* data, uint32_t id, int seq, int res,
                                 const char* message) {
    (void)id;
    (void)seq;
    (void)message;
    struct PlaybackSession* session = data;
    // C API はエラーコードのみを返すため、メッセージは保持しない
    atomic_store(&session->core_error, res);
    pw_thread_loop_signal(session->thread_loop, false);
}

static const struct pw_core_events playback_core_events = {
    PW_VERSION_CORE_EVENTS,
    .error = playback_core_error,
};

struct PlaybackSession* audio_pipewire_playback_session_create(const char* device_id,
                                                               int sample_rate,
                                                               int channels) {
    struct PlaybackSession* session = calloc(1, sizeof(struct PlaybackSession));
    if (!session) {
        return NULL;
    }

    pw_init(NULL, NULL);

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
    atomic_init(&session->core_error, 0);

    // thread loop を作成する
    session->thread_loop = pw_thread_loop_new("shiguredo-audio-playback", NULL);
    if (!session->thread_loop) {
        free(session->device_id);
        free(session);
        return NULL;
    }

    session->context = pw_context_new(
        pw_thread_loop_get_loop(session->thread_loop), NULL, 0);
    if (!session->context) {
        pw_thread_loop_destroy(session->thread_loop);
        free(session->device_id);
        free(session);
        return NULL;
    }

    return session;
}

void audio_pipewire_playback_session_destroy(struct PlaybackSession* session) {
    if (!session) {
        return;
    }

    // 実行中でも破棄できるよう、停止してからリソースを破棄する
    audio_pipewire_playback_session_stop(session);

    pw_context_destroy(session->context);
    pw_thread_loop_destroy(session->thread_loop);

    free(session->device_id);
    free(session);
}

int audio_pipewire_playback_session_start(struct PlaybackSession* session,
                                          AudioPlaybackCallback callback,
                                          void* user_data) {
    if (!session || !callback) {
        return AUDIO_PIPEWIRE_ERROR_INVALID_ARGUMENT;
    }

    if (atomic_load(&session->running)) {
        return AUDIO_PIPEWIRE_RESULT_OK;
    }

    session->callback = callback;
    session->user_data = user_data;

    // thread loop を開始する
    if (pw_thread_loop_start(session->thread_loop) < 0) {
        return AUDIO_PIPEWIRE_ERROR_THREAD_LOOP_START;
    }

    pw_thread_loop_lock(session->thread_loop);

    int result = AUDIO_PIPEWIRE_RESULT_OK;
    struct pw_properties* props = NULL;

    atomic_store(&session->core_error, 0);

    // PipeWire に接続する
    session->core = pw_context_connect(session->context, NULL, 0);
    if (!session->core) {
        result = AUDIO_PIPEWIRE_ERROR_CONTEXT_CONNECT;
        goto fail;
    }

    spa_zero(session->core_listener);
    pw_core_add_listener(session->core, &session->core_listener,
                         &playback_core_events, session);

    // ストリームのプロパティを設定する
    props = pw_properties_new(PW_KEY_MEDIA_TYPE, "Audio",
                              PW_KEY_MEDIA_CATEGORY, "Playback",
                              PW_KEY_MEDIA_ROLE, "Communication", NULL);
    if (!props) {
        result = AUDIO_PIPEWIRE_ERROR_PROPERTIES;
        goto fail;
    }

    if (session->device_id) {
        pw_properties_set(props, PW_KEY_TARGET_OBJECT,
                          session->device_id);
    }

    // ストリームを作成する
    // props の所有権は pw_stream_new に移るため、呼び出し後に解放してはいけない
    session->stream =
        pw_stream_new(session->core, "audio-playback", props);
    if (!session->stream) {
        result = AUDIO_PIPEWIRE_ERROR_STREAM_CREATE;
        goto fail;
    }

    spa_zero(session->stream_listener);
    pw_stream_add_listener(session->stream, &session->stream_listener,
                           &playback_stream_events, session);

    // オーディオフォーマットを設定する
    uint8_t params_buffer[1024];
    struct spa_pod_builder builder =
        SPA_POD_BUILDER_INIT(params_buffer, sizeof(params_buffer));

    struct spa_audio_info_raw audio_info = SPA_AUDIO_INFO_RAW_INIT(
        .format = SPA_AUDIO_FORMAT_S16_LE,
        .rate = session->sample_rate,
        .channels = session->channels);

    const struct spa_pod* params[1];
    params[0] = spa_format_audio_raw_build(&builder, SPA_PARAM_EnumFormat,
                                           &audio_info);

    // 再生ストリームを接続する (PW_DIRECTION_OUTPUT)
    if (pw_stream_connect(session->stream, PW_DIRECTION_OUTPUT, PW_ID_ANY,
                          PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS,
                          params, 1) < 0) {
        result = AUDIO_PIPEWIRE_ERROR_STREAM_CONNECT;
        goto fail;
    }

    result = wait_for_stream_ready(session->thread_loop, session->stream,
                                   &session->core_error);
    if (result < 0) {
        goto fail;
    }

    atomic_store(&session->running, 1);
    pw_thread_loop_unlock(session->thread_loop);

    return AUDIO_PIPEWIRE_RESULT_OK;

fail:
    destroy_session_resources(session->thread_loop, &session->stream,
                              &session->core_listener, &session->core);
    return result;
}

int audio_pipewire_playback_session_stop(struct PlaybackSession* session) {
    if (!session) {
        return -1;
    }

    if (!atomic_load(&session->running) && !session->stream && !session->core) {
        return 0;
    }

    atomic_store(&session->running, 0);

    pw_thread_loop_lock(session->thread_loop);
    destroy_session_resources(session->thread_loop, &session->stream,
                              &session->core_listener, &session->core);
    return 0;
}

int audio_pipewire_playback_session_is_in_callback(struct PlaybackSession* session) {
    if (!session) {
        return 0;
    }
    return pw_thread_loop_in_thread(session->thread_loop) ? 1 : 0;
}

int audio_pipewire_playback_session_sample_rate(struct PlaybackSession* session) {
    if (!session) {
        return 0;
    }
    return session->sample_rate;
}

int audio_pipewire_playback_session_channels(struct PlaybackSession* session) {
    if (!session) {
        return 0;
    }
    return session->channels;
}
