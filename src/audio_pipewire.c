// PipeWire を使った Linux 用オーディオキャプチャ実装

#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <pipewire/pipewire.h>
#include <spa/param/audio/format-utils.h>

#include "audio_pipewire.h"

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
    uint32_t size = spa_buf->datas[0].chunk->size;

    if (size > 0 && session->callback) {
        int bytes_per_sample = 2;  // S16
        int frame_size = bytes_per_sample * session->channels;
        int frames = (int)(size / frame_size);

        if (frames > 0) {
            struct timespec ts;
            clock_gettime(CLOCK_MONOTONIC, &ts);
            int64_t timestamp_us =
                (int64_t)ts.tv_sec * 1000000 + ts.tv_nsec / 1000;

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

// core の done イベント (セッション接続用)
static void session_core_done(void* data, uint32_t id, int seq) {
    (void)id;
    (void)seq;
    struct AudioSession* session = data;
    pw_thread_loop_signal(session->thread_loop, false);
}

// core の error イベント
static void session_core_error(void* data, uint32_t id, int seq, int res,
                               const char* message) {
    (void)id;
    (void)seq;
    (void)res;
    (void)message;
    struct AudioSession* session = data;
    pw_thread_loop_signal(session->thread_loop, false);
}

static const struct pw_core_events session_core_events = {
    PW_VERSION_CORE_EVENTS,
    .done = session_core_done,
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

    if (atomic_load(&session->running)) {
        audio_pipewire_session_stop(session);
    }

    if (session->stream) {
        pw_stream_destroy(session->stream);
    }

    if (session->core) {
        spa_hook_remove(&session->core_listener);
        pw_core_disconnect(session->core);
    }

    if (session->context) {
        pw_context_destroy(session->context);
    }

    if (session->thread_loop) {
        pw_thread_loop_destroy(session->thread_loop);
    }

    free(session->device_id);
    free(session);
}

int audio_pipewire_session_start(struct AudioSession* session,
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

    // thread loop を開始する
    if (pw_thread_loop_start(session->thread_loop) < 0) {
        return -2;
    }

    pw_thread_loop_lock(session->thread_loop);

    // PipeWire に接続する
    session->core = pw_context_connect(session->context, NULL, 0);
    if (!session->core) {
        pw_thread_loop_unlock(session->thread_loop);
        pw_thread_loop_stop(session->thread_loop);
        return -3;
    }

    spa_zero(session->core_listener);
    pw_core_add_listener(session->core, &session->core_listener,
                         &session_core_events, session);

    // ストリームのプロパティを設定する
    struct pw_properties* props =
        pw_properties_new(PW_KEY_MEDIA_TYPE, "Audio",
                          PW_KEY_MEDIA_CATEGORY, "Capture",
                          PW_KEY_MEDIA_ROLE, "Communication", NULL);
    if (!props) {
        pw_core_disconnect(session->core);
        session->core = NULL;
        pw_thread_loop_unlock(session->thread_loop);
        pw_thread_loop_stop(session->thread_loop);
        return -4;
    }

    if (session->device_id) {
        pw_properties_set(props, PW_KEY_TARGET_OBJECT,
                          session->device_id);
    }

    // ストリームを作成する
    session->stream =
        pw_stream_new(session->core, "audio-capture", props);
    if (!session->stream) {
        pw_core_disconnect(session->core);
        session->core = NULL;
        pw_thread_loop_unlock(session->thread_loop);
        pw_thread_loop_stop(session->thread_loop);
        return -4;
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
    int result = pw_stream_connect(
        session->stream, PW_DIRECTION_INPUT, PW_ID_ANY,
        PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS,
        params, 1);

    if (result < 0) {
        pw_stream_destroy(session->stream);
        session->stream = NULL;
        spa_hook_remove(&session->core_listener);
        pw_core_disconnect(session->core);
        session->core = NULL;
        pw_thread_loop_unlock(session->thread_loop);
        pw_thread_loop_stop(session->thread_loop);
        return -5;
    }

    // ストリームが streaming 状態になるまで待機する
    while (1) {
        enum pw_stream_state state = pw_stream_get_state(
            session->stream, NULL);
        if (state == PW_STREAM_STATE_STREAMING) {
            break;
        }
        if (state == PW_STREAM_STATE_PAUSED) {
            pw_stream_set_active(session->stream, true);
            break;
        }
        if (state == PW_STREAM_STATE_ERROR ||
            state == PW_STREAM_STATE_UNCONNECTED) {
            pw_stream_destroy(session->stream);
            session->stream = NULL;
            spa_hook_remove(&session->core_listener);
            pw_core_disconnect(session->core);
            session->core = NULL;
            pw_thread_loop_unlock(session->thread_loop);
            pw_thread_loop_stop(session->thread_loop);
            return -5;
        }
        pw_thread_loop_wait(session->thread_loop);
    }

    atomic_store(&session->running, 1);
    pw_thread_loop_unlock(session->thread_loop);

    return 0;
}

void audio_pipewire_session_stop(struct AudioSession* session) {
    if (!session || !atomic_load(&session->running)) {
        return;
    }

    atomic_store(&session->running, 0);

    pw_thread_loop_lock(session->thread_loop);

    if (session->stream) {
        pw_stream_disconnect(session->stream);
    }

    pw_thread_loop_unlock(session->thread_loop);

    pw_thread_loop_stop(session->thread_loop);
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
    int frames = (int)(max_size / frame_size);

    if (frames > 0 && session->callback) {
        int written = session->callback(session->user_data, data, frames,
                                         session->channels, session->sample_rate,
                                         AUDIO_FORMAT_S16);

        if (written <= 0) {
            memset(data, 0, frames * frame_size);
            spa_buf->datas[0].chunk->size = frames * frame_size;
        } else if (written < frames) {
            int written_bytes = written * frame_size;
            memset((uint8_t*)data + written_bytes, 0, (frames - written) * frame_size);
            spa_buf->datas[0].chunk->size = frames * frame_size;
        } else {
            spa_buf->datas[0].chunk->size = frames * frame_size;
        }
    } else {
        memset(data, 0, frames * frame_size);
        spa_buf->datas[0].chunk->size = frames * frame_size;
    }

    spa_buf->datas[0].chunk->offset = 0;
    spa_buf->datas[0].chunk->stride = frame_size;

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

// 再生用 core done イベント
// ストリーム状態変化 (playback_on_stream_state_changed) でシグナルを送るため、ここでは不要
static void playback_core_done(void* data, uint32_t id, int seq) {
    (void)data;
    (void)id;
    (void)seq;
}

// 再生用 core error イベント
static void playback_core_error(void* data, uint32_t id, int seq, int res,
                                 const char* message) {
    (void)id;
    (void)seq;
    (void)res;
    (void)message;
    struct PlaybackSession* session = data;
    pw_thread_loop_signal(session->thread_loop, false);
}

static const struct pw_core_events playback_core_events = {
    PW_VERSION_CORE_EVENTS,
    .done = playback_core_done,
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

    if (atomic_load(&session->running)) {
        audio_pipewire_playback_session_stop(session);
    }

    if (session->stream) {
        pw_stream_destroy(session->stream);
    }

    if (session->core) {
        spa_hook_remove(&session->core_listener);
        pw_core_disconnect(session->core);
    }

    if (session->context) {
        pw_context_destroy(session->context);
    }

    if (session->thread_loop) {
        pw_thread_loop_destroy(session->thread_loop);
    }

    free(session->device_id);
    free(session);
}

int audio_pipewire_playback_session_start(struct PlaybackSession* session,
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

    // thread loop を開始する
    if (pw_thread_loop_start(session->thread_loop) < 0) {
        return -2;
    }

    pw_thread_loop_lock(session->thread_loop);

    // PipeWire に接続する
    session->core = pw_context_connect(session->context, NULL, 0);
    if (!session->core) {
        pw_thread_loop_unlock(session->thread_loop);
        pw_thread_loop_stop(session->thread_loop);
        return -3;
    }

    spa_zero(session->core_listener);
    pw_core_add_listener(session->core, &session->core_listener,
                         &playback_core_events, session);

    // ストリームのプロパティを設定する
    struct pw_properties* props =
        pw_properties_new(PW_KEY_MEDIA_TYPE, "Audio",
                          PW_KEY_MEDIA_CATEGORY, "Playback",
                          PW_KEY_MEDIA_ROLE, "Communication", NULL);
    if (!props) {
        spa_hook_remove(&session->core_listener);
        pw_core_disconnect(session->core);
        session->core = NULL;
        pw_thread_loop_unlock(session->thread_loop);
        pw_thread_loop_stop(session->thread_loop);
        return -4;
    }

    if (session->device_id) {
        pw_properties_set(props, PW_KEY_TARGET_OBJECT,
                          session->device_id);
    }

    // ストリームを作成する
    session->stream =
        pw_stream_new(session->core, "audio-playback", props);
    if (!session->stream) {
        pw_properties_free(props);
        spa_hook_remove(&session->core_listener);
        pw_core_disconnect(session->core);
        session->core = NULL;
        pw_thread_loop_unlock(session->thread_loop);
        pw_thread_loop_stop(session->thread_loop);
        return -4;
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
    int result = pw_stream_connect(
        session->stream, PW_DIRECTION_OUTPUT, PW_ID_ANY,
        PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS,
        params, 1);

    if (result < 0) {
        pw_stream_destroy(session->stream);
        session->stream = NULL;
        spa_hook_remove(&session->core_listener);
        pw_core_disconnect(session->core);
        session->core = NULL;
        pw_thread_loop_unlock(session->thread_loop);
        pw_thread_loop_stop(session->thread_loop);
        return -5;
    }

    // ストリームが streaming 状態になるまで待機する
    while (1) {
        enum pw_stream_state state = pw_stream_get_state(
            session->stream, NULL);
        if (state == PW_STREAM_STATE_STREAMING) {
            break;
        }
        // PipeWire のバージョンや環境によってはストリームが PAUSED に遷移する。
        // この場合 activate してからループを抜ける。
        // セッションマネージャーが利用可能になれば自動的に streaming に遷移する。
        if (state == PW_STREAM_STATE_PAUSED) {
            pw_stream_set_active(session->stream, true);
            break;
        }
        if (state == PW_STREAM_STATE_ERROR ||
            state == PW_STREAM_STATE_UNCONNECTED) {
            pw_stream_destroy(session->stream);
            session->stream = NULL;
            spa_hook_remove(&session->core_listener);
            pw_core_disconnect(session->core);
            session->core = NULL;
            pw_thread_loop_unlock(session->thread_loop);
            pw_thread_loop_stop(session->thread_loop);
            return -5;
        }
        pw_thread_loop_wait(session->thread_loop);
    }

    atomic_store(&session->running, 1);
    pw_thread_loop_unlock(session->thread_loop);

    return 0;
}

void audio_pipewire_playback_session_stop(struct PlaybackSession* session) {
    if (!session || !atomic_load(&session->running)) {
        return;
    }

    atomic_store(&session->running, 0);

    pw_thread_loop_lock(session->thread_loop);

    if (session->stream) {
        pw_stream_disconnect(session->stream);
    }

    pw_thread_loop_unlock(session->thread_loop);

    pw_thread_loop_stop(session->thread_loop);
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
