#import <AudioToolbox/AudioToolbox.h>
#import <CoreAudio/CoreAudio.h>
#import <Foundation/Foundation.h>
#import <mach/mach_time.h>
#include <stdatomic.h>

#include "audio_coreaudio.h"

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
    AudioQueueRef queue;
    AudioQueueBufferRef buffers[3];
    AudioStreamBasicDescription format;
    AudioFrameCallback callback;
    void* user_data;
    atomic_int running;
};

static void audio_input_callback(void* user_data,
                                  AudioQueueRef queue,
                                  AudioQueueBufferRef buffer,
                                  const AudioTimeStamp* start_time,
                                  UInt32 num_packets,
                                  const AudioStreamPacketDescription* packet_desc
                                      __attribute__((unused))) {
    struct AudioSession* session = (struct AudioSession*)user_data;

    if (session->callback && num_packets > 0) {
        // タイムスタンプをマイクロ秒に変換
        int64_t timestamp_us = 0;
        if (start_time->mFlags & kAudioTimeStampHostTimeValid) {
            // ホストタイムをナノ秒に変換してからマイクロ秒に
            mach_timebase_info_data_t timebase;
            kern_return_t r = mach_timebase_info(&timebase);
            if (r == KERN_SUCCESS) {
              uint64_t nanos =
                  start_time->mHostTime * timebase.numer / timebase.denom;
              timestamp_us = (int64_t)(nanos / 1000);
            }
        }

        int format = (session->format.mBitsPerChannel == 16) ? AUDIO_FORMAT_S16
                                                              : AUDIO_FORMAT_F32;

        session->callback(session->user_data, buffer->mAudioData, num_packets,
                          session->format.mChannelsPerFrame,
                          (int)session->format.mSampleRate, format,
                          timestamp_us);
    }

    // バッファを再エンキュー
    if (atomic_load(&session->running)) {
        AudioQueueEnqueueBuffer(queue, buffer, 0, NULL);
    }
}

// デバイス情報（名前、UID）を取得するヘルパー関数
// 成功時は 0 を返し、deviceName と deviceUID に値を設定する
// 失敗時は -1 を返す
static int get_device_info(AudioDeviceID deviceID, CFStringRef* deviceName,
                            CFStringRef* deviceUID) {
    // デバイス名を取得
    AudioObjectPropertyAddress nameAddress = {
        kAudioDevicePropertyDeviceNameCFString,
        kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain};
    UInt32 nameSize = sizeof(CFStringRef);
    OSStatus status = AudioObjectGetPropertyData(deviceID, &nameAddress, 0,
                                                  NULL, &nameSize, deviceName);

    if (status != noErr || !*deviceName) {
        return -1;
    }

    // デバイス UID を取得
    AudioObjectPropertyAddress uidAddress = {
        kAudioDevicePropertyDeviceUID, kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain};
    UInt32 uidSize = sizeof(CFStringRef);
    status = AudioObjectGetPropertyData(deviceID, &uidAddress, 0, NULL,
                                         &uidSize, deviceUID);

    if (status != noErr || !*deviceUID) {
        CFRelease(*deviceName);
        *deviceName = NULL;
        return -1;
    }

    return 0;
}

// 指定スコープのチャンネル数を取得するヘルパー関数
static int get_channel_count(AudioDeviceID deviceID,
                              AudioObjectPropertyScope scope) {
    AudioObjectPropertyAddress channelAddress = {
        kAudioDevicePropertyStreamConfiguration, scope,
        kAudioObjectPropertyElementMain};
    UInt32 channelSize = 0;
    OSStatus status = AudioObjectGetPropertyDataSize(
        deviceID, &channelAddress, 0, NULL, &channelSize);

    int totalChannels = 0;
    if (status == noErr && channelSize > 0) {
        AudioBufferList* bufferList = (AudioBufferList*)malloc(channelSize);
        if (bufferList) {
            status = AudioObjectGetPropertyData(
                deviceID, &channelAddress, 0, NULL, &channelSize, bufferList);
            if (status == noErr) {
                for (UInt32 j = 0; j < bufferList->mNumberBuffers; j++) {
                    totalChannels += bufferList->mBuffers[j].mNumberChannels;
                }
            }
            free(bufferList);
        }
    }

    return totalChannels;
}

// デバイスを動的配列に追加するヘルパー関数
// 成功時は 0、メモリ確保失敗時は -1 を返す
static int add_device_to_array(struct AudioDevice*** deviceArray,
                                int* deviceCount, int* capacity,
                                CFStringRef deviceName,
                                CFStringRef deviceUID,
                                int channels, int sample_rate,
                                int device_type) {
    // 配列を拡張
    if (*deviceCount >= *capacity) {
        int new_capacity = *capacity == 0 ? 8 : *capacity * 2;
        struct AudioDevice** newArray = (struct AudioDevice**)realloc(
            *deviceArray, sizeof(struct AudioDevice*) * new_capacity);
        if (!newArray) {
            return -1;
        }
        *deviceArray = newArray;
        *capacity = new_capacity;
    }

    struct AudioDevice* device =
        (struct AudioDevice*)malloc(sizeof(struct AudioDevice));
    if (!device) {
        return -1;
    }

    // CFString を C 文字列に変換
    CFIndex nameLength =
        CFStringGetMaximumSizeForEncoding(
            CFStringGetLength(deviceName), kCFStringEncodingUTF8) +
        1;
    device->name = (char*)malloc(nameLength);
    if (!device->name) {
        free(device);
        return -1;
    }
    if (!CFStringGetCString(deviceName, device->name, nameLength,
                             kCFStringEncodingUTF8)) {
        free(device->name);
        free(device);
        return -1;
    }

    CFIndex uidLength =
        CFStringGetMaximumSizeForEncoding(CFStringGetLength(deviceUID),
                                           kCFStringEncodingUTF8) +
        1;
    device->unique_id = (char*)malloc(uidLength);
    if (!device->unique_id) {
        free(device->name);
        free(device);
        return -1;
    }
    if (!CFStringGetCString(deviceUID, device->unique_id, uidLength,
                             kCFStringEncodingUTF8)) {
        free(device->unique_id);
        free(device->name);
        free(device);
        return -1;
    }

    device->channels = channels > 0 ? channels : 2;
    device->sample_rate = sample_rate > 0 ? sample_rate : 48000;
    device->device_type = device_type;

    (*deviceArray)[(*deviceCount)++] = device;
    return 0;
}

int audio_coreaudio_enumerate_devices(struct AudioDevice*** devices, int* count) {
    if (!devices || !count) {
        return -1;
    }

    // 全デバイスの取得
    AudioObjectPropertyAddress propertyAddress = {
        kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain};

    UInt32 dataSize = 0;
    OSStatus status = AudioObjectGetPropertyDataSize(
        kAudioObjectSystemObject, &propertyAddress, 0, NULL, &dataSize);

    if (status != noErr) {
        return -1;
    }

    UInt32 deviceCount = dataSize / sizeof(AudioDeviceID);
    if (deviceCount == 0) {
        *devices = NULL;
        *count = 0;
        return 0;
    }

    AudioDeviceID* deviceIDs = (AudioDeviceID*)malloc(dataSize);
    if (!deviceIDs) {
        return -2;
    }

    status = AudioObjectGetPropertyData(kAudioObjectSystemObject,
                                         &propertyAddress, 0, NULL, &dataSize,
                                         deviceIDs);

    if (status != noErr) {
        free(deviceIDs);
        return -1;
    }

    struct AudioDevice** deviceArray = NULL;
    int totalDeviceCount = 0;
    int capacity = 0;

    for (UInt32 i = 0; i < deviceCount; i++) {
        // 入力ストリームがあるかチェック
        AudioObjectPropertyAddress inputStreamAddress = {
            kAudioDevicePropertyStreams, kAudioDevicePropertyScopeInput,
            kAudioObjectPropertyElementMain};
        UInt32 inputStreamSize = 0;
        status = AudioObjectGetPropertyDataSize(
            deviceIDs[i], &inputStreamAddress, 0, NULL, &inputStreamSize);
        int has_input = (status == noErr && inputStreamSize > 0);

        // 出力ストリームがあるかチェック
        AudioObjectPropertyAddress outputStreamAddress = {
            kAudioDevicePropertyStreams, kAudioDevicePropertyScopeOutput,
            kAudioObjectPropertyElementMain};
        UInt32 outputStreamSize = 0;
        status = AudioObjectGetPropertyDataSize(
            deviceIDs[i], &outputStreamAddress, 0, NULL, &outputStreamSize);
        int has_output = (status == noErr && outputStreamSize > 0);

        if (!has_input && !has_output) {
            continue;
        }

        // デバイス情報を取得
        CFStringRef deviceName = NULL;
        CFStringRef deviceUID = NULL;
        if (get_device_info(deviceIDs[i], &deviceName, &deviceUID) < 0) {
            continue;
        }

        // サンプルレートを取得
        Float64 sampleRate = 0;
        AudioObjectPropertyAddress rateAddress = {
            kAudioDevicePropertyNominalSampleRate,
            kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain};
        UInt32 rateSize = sizeof(Float64);
        AudioObjectGetPropertyData(deviceIDs[i], &rateAddress, 0, NULL,
                                    &rateSize, &sampleRate);

        // 入力デバイスとして追加
        if (has_input) {
            int inputChannels = get_channel_count(
                deviceIDs[i], kAudioDevicePropertyScopeInput);
            if (add_device_to_array(&deviceArray, &totalDeviceCount,
                                     &capacity, deviceName,
                                     deviceUID, inputChannels,
                                     (int)sampleRate,
                                     AUDIO_DEVICE_TYPE_INPUT) < 0) {
                CFRelease(deviceName);
                CFRelease(deviceUID);
                for (int j = 0; j < totalDeviceCount; j++) {
                    free(deviceArray[j]->name);
                    free(deviceArray[j]->unique_id);
                    free(deviceArray[j]);
                }
                free(deviceArray);
                free(deviceIDs);
                return -2;
            }
        }

        // 出力デバイスとして追加
        if (has_output) {
            int outputChannels = get_channel_count(
                deviceIDs[i], kAudioDevicePropertyScopeOutput);
            if (add_device_to_array(&deviceArray, &totalDeviceCount,
                                     &capacity, deviceName,
                                     deviceUID, outputChannels,
                                     (int)sampleRate,
                                     AUDIO_DEVICE_TYPE_OUTPUT) < 0) {
                CFRelease(deviceName);
                CFRelease(deviceUID);
                for (int j = 0; j < totalDeviceCount; j++) {
                    free(deviceArray[j]->name);
                    free(deviceArray[j]->unique_id);
                    free(deviceArray[j]);
                }
                free(deviceArray);
                free(deviceIDs);
                return -2;
            }
        }

        CFRelease(deviceName);
        CFRelease(deviceUID);
    }

    free(deviceIDs);

    *devices = deviceArray;
    *count = totalDeviceCount;
    return 0;
}

void audio_coreaudio_free_devices(struct AudioDevice** devices, int count) {
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

const char* audio_coreaudio_device_name(struct AudioDevice* device) {
    if (!device) {
        return NULL;
    }
    return device->name;
}

const char* audio_coreaudio_device_unique_id(struct AudioDevice* device) {
    if (!device) {
        return NULL;
    }
    return device->unique_id;
}

int audio_coreaudio_device_channels(struct AudioDevice* device) {
    if (!device) {
        return 0;
    }
    return device->channels;
}

int audio_coreaudio_device_sample_rate(struct AudioDevice* device) {
    if (!device) {
        return 0;
    }
    return device->sample_rate;
}

int audio_coreaudio_device_type(struct AudioDevice* device) {
    if (!device) {
        return AUDIO_DEVICE_TYPE_INPUT;
    }
    return device->device_type;
}

static AudioDeviceID find_device_by_uid(const char* uid) {
    AudioObjectPropertyAddress propertyAddress = {
        kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain};

    UInt32 dataSize = 0;
    OSStatus status = AudioObjectGetPropertyDataSize(
        kAudioObjectSystemObject, &propertyAddress, 0, NULL, &dataSize);

    if (status != noErr) {
        return kAudioObjectUnknown;
    }

    UInt32 deviceCount = dataSize / sizeof(AudioDeviceID);
    AudioDeviceID* deviceIDs = (AudioDeviceID*)malloc(dataSize);
    if (!deviceIDs) {
        return kAudioObjectUnknown;
    }

    status = AudioObjectGetPropertyData(kAudioObjectSystemObject,
                                         &propertyAddress, 0, NULL, &dataSize,
                                         deviceIDs);

    if (status != noErr) {
        free(deviceIDs);
        return kAudioObjectUnknown;
    }

    AudioDeviceID foundDevice = kAudioObjectUnknown;
    NSString* targetUID = [NSString stringWithUTF8String:uid];

    for (UInt32 i = 0; i < deviceCount; i++) {
        CFStringRef deviceUID = NULL;
        AudioObjectPropertyAddress uidAddress = {
            kAudioDevicePropertyDeviceUID, kAudioObjectPropertyScopeGlobal,
            kAudioObjectPropertyElementMain};
        UInt32 uidSize = sizeof(CFStringRef);
        status = AudioObjectGetPropertyData(deviceIDs[i], &uidAddress, 0, NULL,
                                             &uidSize, &deviceUID);

        if (status == noErr && deviceUID) {
            if ([(__bridge NSString*)deviceUID isEqualToString:targetUID]) {
                foundDevice = deviceIDs[i];
                CFRelease(deviceUID);
                break;
            }
            CFRelease(deviceUID);
        }
    }

    free(deviceIDs);
    return foundDevice;
}

struct AudioSession* audio_coreaudio_session_create(const char* device_id,
                                           int sample_rate,
                                           int channels) {
    struct AudioSession* session =
        (struct AudioSession*)calloc(1, sizeof(struct AudioSession));
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

    // オーディオフォーマットの設定（16-bit signed integer）
    session->format.mSampleRate = sample_rate;
    session->format.mFormatID = kAudioFormatLinearPCM;
    session->format.mFormatFlags =
        kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked;
    session->format.mBitsPerChannel = 16;
    session->format.mChannelsPerFrame = channels;
    session->format.mBytesPerFrame =
        session->format.mChannelsPerFrame * (session->format.mBitsPerChannel / 8);
    session->format.mFramesPerPacket = 1;
    session->format.mBytesPerPacket = session->format.mBytesPerFrame;

    // AudioQueue を作成
    OSStatus status = AudioQueueNewInput(
        &session->format, audio_input_callback, session, NULL,
        kCFRunLoopCommonModes, 0, &session->queue);

    if (status != noErr) {
        free(session);
        return NULL;
    }

    // デバイスを設定
    if (device_id) {
        AudioDeviceID deviceID = find_device_by_uid(device_id);
        if (deviceID != kAudioObjectUnknown) {
            CFStringRef deviceUID =
                CFStringCreateWithCString(NULL, device_id, kCFStringEncodingUTF8);
            if (deviceUID) {
                AudioQueueSetProperty(session->queue,
                                       kAudioQueueProperty_CurrentDevice,
                                       &deviceUID, sizeof(CFStringRef));
                CFRelease(deviceUID);
            }
        }
    }

    // バッファを作成（10ms 分）
    UInt32 bufferByteSize =
        session->format.mSampleRate * session->format.mBytesPerFrame / 100;

    for (int i = 0; i < 3; i++) {
        status = AudioQueueAllocateBuffer(session->queue, bufferByteSize,
                                           &session->buffers[i]);
        if (status != noErr) {
            AudioQueueDispose(session->queue, true);
            free(session);
            return NULL;
        }
    }

    return session;
}

void audio_coreaudio_session_destroy(struct AudioSession* session) {
    if (!session) {
        return;
    }

    if (atomic_load(&session->running)) {
        audio_coreaudio_session_stop(session);
    }

    AudioQueueDispose(session->queue, true);
    free(session);
}

int audio_coreaudio_session_start(struct AudioSession* session,
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
    atomic_store(&session->running, 1);

    // バッファをエンキュー
    for (int i = 0; i < 3; i++) {
        OSStatus status =
            AudioQueueEnqueueBuffer(session->queue, session->buffers[i], 0, NULL);
        if (status != noErr) {
            atomic_store(&session->running, 0);
            return -1;
        }
    }

    // キャプチャを開始
    OSStatus status = AudioQueueStart(session->queue, NULL);
    if (status != noErr) {
        atomic_store(&session->running, 0);
        return -1;
    }

    return 0;
}

void audio_coreaudio_session_stop(struct AudioSession* session) {
    if (!session) {
        return;
    }

    if (!atomic_load(&session->running)) {
        return;
    }

    atomic_store(&session->running, 0);
    AudioQueueStop(session->queue, true);
}

int audio_coreaudio_session_sample_rate(struct AudioSession* session) {
    if (!session) {
        return 0;
    }
    return (int)session->format.mSampleRate;
}

int audio_coreaudio_session_channels(struct AudioSession* session) {
    if (!session) {
        return 0;
    }
    return session->format.mChannelsPerFrame;
}

// --- 再生 (Playback) 実装 ---

struct PlaybackSession {
    AudioQueueRef queue;
    AudioQueueBufferRef buffers[3];
    AudioStreamBasicDescription format;
    AudioPlaybackCallback callback;
    void* user_data;
    atomic_int running;
};

static void audio_output_callback(void* user_data,
                                   AudioQueueRef queue,
                                   AudioQueueBufferRef buffer) {
    struct PlaybackSession* session = (struct PlaybackSession*)user_data;

    if (!atomic_load(&session->running) || !session->callback) {
        // running=0 の場合はエンキューしない（AudioQueueStop を待つ）
        memset(buffer->mAudioData, 0, buffer->mAudioDataByteSize);
        return;
    }

    int bytes_per_frame = session->format.mBytesPerFrame;
    int frames = (int)(buffer->mAudioDataByteSize / bytes_per_frame);
    int format = (session->format.mBitsPerChannel == 16) ? AUDIO_FORMAT_S16
                                                          : AUDIO_FORMAT_F32;

    int written = session->callback(session->user_data,
                                     buffer->mAudioData,
                                     frames,
                                     session->format.mChannelsPerFrame,
                                     (int)session->format.mSampleRate,
                                     format);

    if (written > frames) {
        written = frames;
    }

    if (written <= 0) {
        // コールバックがデータを返さなかった場合は無音で埋める
        memset(buffer->mAudioData, 0, buffer->mAudioDataByteSize);
    } else if (written < frames) {
        // 残りを無音で埋める
        int written_bytes = written * bytes_per_frame;
        int remaining_bytes = (int)buffer->mAudioDataByteSize - written_bytes;
        memset((uint8_t*)buffer->mAudioData + written_bytes, 0, remaining_bytes);
    }

    AudioQueueEnqueueBuffer(queue, buffer, 0, NULL);
}

struct PlaybackSession* audio_coreaudio_playback_session_create(const char* device_id,
                                                                int sample_rate,
                                                                int channels) {
    struct PlaybackSession* session =
        (struct PlaybackSession*)calloc(1, sizeof(struct PlaybackSession));
    if (!session) {
        return NULL;
    }

    atomic_init(&session->running, 0);

    // デフォルト値の設定
    if (sample_rate <= 0) {
        sample_rate = 48000;
    }
    if (channels <= 0) {
        channels = 2;
    }

    // オーディオフォーマットの設定（16-bit signed integer）
    session->format.mSampleRate = sample_rate;
    session->format.mFormatID = kAudioFormatLinearPCM;
    session->format.mFormatFlags =
        kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked;
    session->format.mBitsPerChannel = 16;
    session->format.mChannelsPerFrame = channels;
    session->format.mBytesPerFrame =
        session->format.mChannelsPerFrame * (session->format.mBitsPerChannel / 8);
    session->format.mFramesPerPacket = 1;
    session->format.mBytesPerPacket = session->format.mBytesPerFrame;

    // AudioQueue (出力) を作成
    OSStatus status = AudioQueueNewOutput(
        &session->format, audio_output_callback, session, NULL,
        kCFRunLoopCommonModes, 0, &session->queue);

    if (status != noErr) {
        free(session);
        return NULL;
    }

    // デバイスを設定
    if (device_id) {
        AudioDeviceID deviceID = find_device_by_uid(device_id);
        if (deviceID == kAudioObjectUnknown) {
            AudioQueueDispose(session->queue, true);
            free(session);
            return NULL;
        }
        CFStringRef deviceUID =
            CFStringCreateWithCString(NULL, device_id, kCFStringEncodingUTF8);
        if (!deviceUID) {
            AudioQueueDispose(session->queue, true);
            free(session);
            return NULL;
        }
        OSStatus setStatus = AudioQueueSetProperty(session->queue,
                                   kAudioQueueProperty_CurrentDevice,
                                   &deviceUID, sizeof(CFStringRef));
        CFRelease(deviceUID);
        if (setStatus != noErr) {
            AudioQueueDispose(session->queue, true);
            free(session);
            return NULL;
        }
    }

    // バッファを作成（10ms 分 x 3）
    // mSampleRate は整数値のサンプルレートなので直接キャストする
    UInt32 bufferByteSize =
        ((UInt32)session->format.mSampleRate * session->format.mBytesPerFrame) /
        100;

    for (int i = 0; i < 3; i++) {
        status = AudioQueueAllocateBuffer(session->queue, bufferByteSize,
                                           &session->buffers[i]);
        if (status != noErr) {
            AudioQueueDispose(session->queue, true);
            free(session);
            return NULL;
        }
        session->buffers[i]->mAudioDataByteSize = bufferByteSize;
    }

    return session;
}

void audio_coreaudio_playback_session_destroy(struct PlaybackSession* session) {
    if (!session) {
        return;
    }

    if (atomic_load(&session->running)) {
        audio_coreaudio_playback_session_stop(session);
    }

    AudioQueueDispose(session->queue, true);
    free(session);
}

int audio_coreaudio_playback_session_start(struct PlaybackSession* session,
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
    atomic_store(&session->running, 1);

    // 無音でバッファを初期化してエンキュー
    for (int i = 0; i < 3; i++) {
        memset(session->buffers[i]->mAudioData, 0,
               session->buffers[i]->mAudioDataByteSize);
        OSStatus status =
            AudioQueueEnqueueBuffer(session->queue, session->buffers[i], 0, NULL);
        if (status != noErr) {
            atomic_store(&session->running, 0);
            return -1;
        }
    }

    // 再生を開始
    OSStatus status = AudioQueueStart(session->queue, NULL);
    if (status != noErr) {
        atomic_store(&session->running, 0);
        return -1;
    }

    return 0;
}

void audio_coreaudio_playback_session_stop(struct PlaybackSession* session) {
    if (!session || !atomic_load(&session->running)) {
        return;
    }

    atomic_store(&session->running, 0);
    AudioQueueStop(session->queue, true);
}

int audio_coreaudio_playback_session_sample_rate(struct PlaybackSession* session) {
    if (!session) {
        return 0;
    }
    return (int)session->format.mSampleRate;
}

int audio_coreaudio_playback_session_channels(struct PlaybackSession* session) {
    if (!session) {
        return 0;
    }
    return session->format.mChannelsPerFrame;
}
