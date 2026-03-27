#define _CRT_SECURE_NO_WARNINGS

#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>

typedef unsigned long ULONG;

#define STATUS_NOERROR 0x00UL
#define ERR_FAILED 0x07UL
#define ERR_BUFFER_EMPTY 0x10UL

#define PROTOCOL_CAN 0x00000005UL
#define J1962_CONNECTOR 0x00000001UL
#define CAN_29BIT_ID 0x00000100UL
#define CAN_ID_BOTH 0x00000800UL
#define PASS_FILTER 0x00000001UL

typedef struct {
    ULONG Connector;
    ULONG NumOfResources;
    ULONG* ResourceListPtr;
} RESOURCE_STRUCT;

/* v05.00 PASSTHRU_MSG — pack(1) to match SAE J2534-2 layout.
 * Without pack(1), MSVC x64 inserts 4 bytes of padding before the
 * DataBuffer pointer (offset 32 instead of 28), which crashes DLLs
 * compiled with the packed layout.  */
#pragma pack(push, 1)
typedef struct {
    ULONG ProtocolID;
    ULONG MsgHandle;
    ULONG RxStatus;
    ULONG TxFlags;
    ULONG Timestamp;
    ULONG DataLength;
    ULONG ExtraDataIndex;
    unsigned char* DataBuffer;
    ULONG DataBufferSize;
} PASSTHRU_MSG;
#pragma pack(pop)

typedef struct {
    ULONG ProtocolID;
    ULONG RxStatus;
    ULONG TxFlags;
    ULONG Timestamp;
    ULONG DataSize;
    ULONG ExtraDataIndex;
    unsigned char Data[4128];
} PASSTHRU_MSG_404;

typedef struct {
    char DeviceName[80];
    ULONG DeviceAvailable;
    ULONG DeviceDLLFWStatus;
    ULONG DeviceConnectMedia;
    ULONG DeviceConnectSpeed;
    ULONG DeviceSignalQuality;
    ULONG DeviceSignalStrength;
} SDEVICE;

typedef long(__stdcall* PassThruOpenFn)(const char* pName, ULONG* pDeviceId);
typedef long(__stdcall* PassThruCloseFn)(ULONG deviceId);
typedef long(__stdcall* PassThruConnectFn)(ULONG deviceId, ULONG protocolId, ULONG flags, ULONG baudRate, RESOURCE_STRUCT resourceStruct, ULONG* pChannelId);
typedef long(__stdcall* PassThruDisconnectFn)(ULONG channelId);
typedef long(__stdcall* PassThruReadMsgsFn)(ULONG channelId, PASSTHRU_MSG* pMsg, ULONG* pNumMsgs, ULONG timeout);
typedef long(__stdcall* PassThruStartMsgFilterFn)(ULONG channelId, ULONG filterType, const PASSTHRU_MSG* pMaskMsg, const PASSTHRU_MSG* pPatternMsg, ULONG* pFilterId);
typedef long(__stdcall* PassThruConnectFn404)(ULONG deviceId, ULONG protocolId, ULONG flags, ULONG baudRate, ULONG* pChannelId);
typedef long(__stdcall* PassThruReadMsgsFn404)(ULONG channelId, PASSTHRU_MSG_404* pMsg, ULONG* pNumMsgs, ULONG timeout);
typedef long(__stdcall* PassThruStartMsgFilterFn404)(ULONG channelId, ULONG filterType, const PASSTHRU_MSG_404* pMaskMsg, const PASSTHRU_MSG_404* pPatternMsg, const PASSTHRU_MSG_404* pFlowControlMsg, ULONG* pFilterId);
/* v5 struct but old 6-param calling convention (hypothesis: PEAK forgot to drop pFlowControlMsg) */
typedef long(__stdcall* PassThruStartMsgFilterFn_V5_6Param)(ULONG channelId, ULONG filterType, const PASSTHRU_MSG* pMaskMsg, const PASSTHRU_MSG* pPatternMsg, const PASSTHRU_MSG* pFlowControlMsg, ULONG* pFilterId);
typedef long(__stdcall* PassThruGetLastErrorFn)(char* pErrorDescription);
typedef long(__stdcall* PassThruScanForDevicesFn)(ULONG* pDeviceCount);
typedef long(__stdcall* PassThruGetNextDeviceFn)(SDEVICE* pDevice);

typedef struct {
    HMODULE module;
    PassThruOpenFn PassThruOpen;
    PassThruCloseFn PassThruClose;
    PassThruConnectFn PassThruConnect;
    PassThruDisconnectFn PassThruDisconnect;
    PassThruReadMsgsFn PassThruReadMsgs;
    PassThruStartMsgFilterFn PassThruStartMsgFilter;
    PassThruGetLastErrorFn PassThruGetLastError;
    PassThruScanForDevicesFn PassThruScanForDevices;
    PassThruGetNextDeviceFn PassThruGetNextDevice;
} Api;

typedef struct {
    const char* dll_path;
    const char* open_name;
    ULONG baud;
    ULONG connect_flags;
    ULONG timeout_ms;
    unsigned duration_secs;
    int install_filter;
} RunConfig;

static long safe_start_msg_filter(
    Api* api,
    ULONG channel_id,
    ULONG filter_type,
    const PASSTHRU_MSG* mask_msg,
    const PASSTHRU_MSG* pattern_msg,
    ULONG* filter_id,
    DWORD* seh_code
) {
    long status = ERR_FAILED;
    *seh_code = 0;
#if defined(_MSC_VER)
    __try {
        status = api->PassThruStartMsgFilter(channel_id, filter_type, mask_msg, pattern_msg, filter_id);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        *seh_code = GetExceptionCode();
    }
#else
    status = api->PassThruStartMsgFilter(channel_id, filter_type, mask_msg, pattern_msg, filter_id);
#endif
    return status;
}

static long safe_read_msgs(
    Api* api,
    ULONG channel_id,
    PASSTHRU_MSG* msgs,
    ULONG* num_msgs,
    ULONG timeout_ms,
    DWORD* seh_code
) {
    long status = ERR_FAILED;
    *seh_code = 0;
#if defined(_MSC_VER)
    __try {
        status = api->PassThruReadMsgs(channel_id, msgs, num_msgs, timeout_ms);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        *seh_code = GetExceptionCode();
    }
#else
    status = api->PassThruReadMsgs(channel_id, msgs, num_msgs, timeout_ms);
#endif
    return status;
}

static void logf(const char* fmt, ...) {
    va_list args;
    va_start(args, fmt);
    vfprintf(stderr, fmt, args);
    fputc('\n', stderr);
    va_end(args);
}

static void usage(const char* argv0) {
    fprintf(stderr,
        "Usage:\n"
        "  %s list [--dll PATH]\n"
        "  %s can-rx [--dll PATH] [--open-name NAME] [--baud 500000] [--duration 10]\n"
        "  %s can-rx-legacy [--dll PATH] [--open-name NAME] [--baud 500000] [--duration 10]\n"
        "  %s can-rx-hybrid [--dll PATH] ...    (v5 connect + v4.04 filter/read)\n"
        "  %s can-rx-v5-6param [--dll PATH] ... (v5 connect + 6-param filter w/ v5 struct)\n"
        "            [--timeout-ms 250] [--connect-flags 0x0] [--skip-filter]\n",
        argv0, argv0, argv0, argv0, argv0);
}

static int parse_ulong_arg(const char* text, ULONG* out) {
    char* end = NULL;
    unsigned long value = strtoul(text, &end, 0);
    if (text[0] == '\0' || end == text || *end != '\0') {
        return 0;
    }
    *out = (ULONG)value;
    return 1;
}

static int load_api(Api* api, const char* dll_path) {
    memset(api, 0, sizeof(*api));
    api->module = LoadLibraryA(dll_path);
    if (!api->module) {
        logf("LoadLibrary failed for '%s' (winerr=%lu)", dll_path, GetLastError());
        return 0;
    }

    api->PassThruOpen = (PassThruOpenFn)GetProcAddress(api->module, "PassThruOpen");
    api->PassThruClose = (PassThruCloseFn)GetProcAddress(api->module, "PassThruClose");
    api->PassThruConnect = (PassThruConnectFn)GetProcAddress(api->module, "PassThruConnect");
    api->PassThruDisconnect = (PassThruDisconnectFn)GetProcAddress(api->module, "PassThruDisconnect");
    api->PassThruReadMsgs = (PassThruReadMsgsFn)GetProcAddress(api->module, "PassThruReadMsgs");
    api->PassThruStartMsgFilter = (PassThruStartMsgFilterFn)GetProcAddress(api->module, "PassThruStartMsgFilter");
    api->PassThruGetLastError = (PassThruGetLastErrorFn)GetProcAddress(api->module, "PassThruGetLastError");
    api->PassThruScanForDevices = (PassThruScanForDevicesFn)GetProcAddress(api->module, "PassThruScanForDevices");
    api->PassThruGetNextDevice = (PassThruGetNextDeviceFn)GetProcAddress(api->module, "PassThruGetNextDevice");

    if (!api->PassThruOpen || !api->PassThruClose || !api->PassThruConnect || !api->PassThruDisconnect || !api->PassThruReadMsgs) {
        logf("Required J2534 exports are missing in '%s'", dll_path);
        FreeLibrary(api->module);
        memset(api, 0, sizeof(*api));
        return 0;
    }

    return 1;
}

static void unload_api(Api* api) {
    if (api->module) {
        FreeLibrary(api->module);
    }
    memset(api, 0, sizeof(*api));
}

static void print_last_error(Api* api, const char* prefix, long status) {
    char buffer[512];
    buffer[0] = '\0';
    if (api->PassThruGetLastError && api->PassThruGetLastError(buffer) == STATUS_NOERROR && buffer[0] != '\0') {
        logf("%s status=%ld last_error=\"%s\"", prefix, status, buffer);
    } else {
        logf("%s status=%ld", prefix, status);
    }
}

static int scan_devices(Api* api, SDEVICE* devices, size_t capacity, size_t* out_count) {
    size_t count = 0;
    ULONG reported_count = 0;
    long status;

    *out_count = 0;
    if (!api->PassThruScanForDevices || !api->PassThruGetNextDevice) {
        logf("This DLL does not export PassThruScanForDevices/GetNextDevice");
        return 0;
    }

    status = api->PassThruScanForDevices(&reported_count);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruScanForDevices failed", status);
        return 0;
    }

    for (;;) {
        SDEVICE device;
        memset(&device, 0, sizeof(device));
        status = api->PassThruGetNextDevice(&device);
        if (status == ERR_BUFFER_EMPTY || status == 0x0CUL) {
            break;
        }
        if (status != STATUS_NOERROR) {
            print_last_error(api, "PassThruGetNextDevice failed", status);
            return 0;
        }
        if (count < capacity) {
            devices[count] = device;
        }
        count++;
    }

    *out_count = count;
    logf("PassThruScanForDevices reported %lu device(s)", reported_count);
    return 1;
}

static int list_mode(Api* api) {
    SDEVICE devices[32];
    size_t count = 0;
    size_t i;

    if (!scan_devices(api, devices, sizeof(devices) / sizeof(devices[0]), &count)) {
        return 1;
    }

    printf("Devices found: %zu\n", count);
    for (i = 0; i < count && i < (sizeof(devices) / sizeof(devices[0])); ++i) {
        printf("[%zu] available=%lu dll_fw=%lu media=%lu speed=%lu signal_quality=%lu signal_strength=%lu name=%s\n",
            i,
            devices[i].DeviceAvailable,
            devices[i].DeviceDLLFWStatus,
            devices[i].DeviceConnectMedia,
            devices[i].DeviceConnectSpeed,
            devices[i].DeviceSignalQuality,
            devices[i].DeviceSignalStrength,
            devices[i].DeviceName);
    }
    return 0;
}

static int install_pass_all_filter(Api* api, ULONG channel_id) {
    unsigned char mask_bytes[4] = {0x00, 0x00, 0xFF, 0xFF};
    unsigned char pattern_bytes[4] = {0x00, 0x00, 0x01, 0x40};
    PASSTHRU_MSG mask_msg;
    PASSTHRU_MSG pattern_msg;
    ULONG filter_id = 0;
    long status;
    DWORD exception_code = 0;

    if (!api->PassThruStartMsgFilter) {
        logf("PassThruStartMsgFilter export missing; continuing without a filter");
        return 1;
    }

    memset(&mask_msg, 0, sizeof(mask_msg));
    memset(&pattern_msg, 0, sizeof(pattern_msg));
    mask_msg.ProtocolID = PROTOCOL_CAN;
    mask_msg.DataBuffer = mask_bytes;
    mask_msg.DataBufferSize = 4;
    mask_msg.DataLength = 4;
    mask_msg.ExtraDataIndex = 4;
    pattern_msg.ProtocolID = PROTOCOL_CAN;
    pattern_msg.DataBuffer = pattern_bytes;
    pattern_msg.DataBufferSize = 4;
    pattern_msg.DataLength = 4;
    pattern_msg.ExtraDataIndex = 4;

    logf("Installing PASS_FILTER on channel=%lu mask=00 00 FF FF pattern=00 00 01 40 txflags=0x%08lX", channel_id, mask_msg.TxFlags);
    status = safe_start_msg_filter(
        api,
        channel_id,
        PASS_FILTER,
        &mask_msg,
        &pattern_msg,
        &filter_id,
        &exception_code
    );
    if (exception_code != 0) {
        logf("PassThruStartMsgFilter raised exception 0x%08lX", exception_code);
        return 0;
    }
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruStartMsgFilter failed", status);
        return 0;
    }

    logf("Installed PASS_FILTER id=%lu for 11-bit traffic", filter_id);
    return 1;
}

static int open_device(Api* api, const char* open_name, ULONG* device_id) {
    long status = api->PassThruOpen(open_name, device_id);
    if (status == STATUS_NOERROR) {
        logf("PassThruOpen succeeded (name=%s device_id=%lu)", open_name ? open_name : "NULL", *device_id);
        return 1;
    }

    print_last_error(api, "PassThruOpen failed", status);
    return 0;
}

static int try_open_with_fallback(Api* api, const char* open_name, ULONG* device_id) {
    SDEVICE devices[32];
    size_t count = 0;

    if (open_name) {
        return open_device(api, open_name, device_id);
    }

    if (open_device(api, NULL, device_id)) {
        return 1;
    }

    if (!scan_devices(api, devices, sizeof(devices) / sizeof(devices[0]), &count) || count == 0) {
        return 0;
    }

    logf("Retrying PassThruOpen with first scanned device: %s", devices[0].DeviceName);
    return open_device(api, devices[0].DeviceName, device_id);
}

static void print_can_frame(const PASSTHRU_MSG* msg) {
    ULONG arb_id;
    ULONG payload_len;
    ULONG i;
    int extended;

    if (msg->DataLength < 4 || !msg->DataBuffer) {
        printf("short-msg len=%lu rx=0x%08lX ts=%lu\n", msg->DataLength, msg->RxStatus, msg->Timestamp);
        return;
    }

    arb_id = ((ULONG)msg->DataBuffer[0] << 24) |
             ((ULONG)msg->DataBuffer[1] << 16) |
             ((ULONG)msg->DataBuffer[2] << 8) |
             (ULONG)msg->DataBuffer[3];
    payload_len = msg->DataLength - 4;
    extended = (msg->RxStatus & CAN_29BIT_ID) != 0 || arb_id > 0x7FFUL;

    printf("ts_us=%-10lu id=0x%08lX kind=%s dlc=%lu data=",
        msg->Timestamp,
        arb_id,
        extended ? "ext" : "std",
        payload_len);
    for (i = 0; i < payload_len; ++i) {
        printf("%02X", msg->DataBuffer[4 + i]);
        if (i + 1 < payload_len) {
            putchar(' ');
        }
    }
    printf(" rx=0x%08lX\n", msg->RxStatus);
}

static void print_can_frame_404(const PASSTHRU_MSG_404* msg) {
    ULONG arb_id;
    ULONG payload_len;
    ULONG i;
    int extended;

    if (msg->DataSize < 4) {
        printf("short-msg len=%lu rx=0x%08lX ts=%lu\n", msg->DataSize, msg->RxStatus, msg->Timestamp);
        return;
    }

    arb_id = ((ULONG)msg->Data[0] << 24) |
             ((ULONG)msg->Data[1] << 16) |
             ((ULONG)msg->Data[2] << 8) |
             (ULONG)msg->Data[3];
    payload_len = msg->DataSize - 4;
    extended = (msg->RxStatus & CAN_29BIT_ID) != 0 || arb_id > 0x7FFUL;

    printf("ts_us=%-10lu id=0x%08lX kind=%s dlc=%lu data=",
        msg->Timestamp,
        arb_id,
        extended ? "ext" : "std",
        payload_len);
    for (i = 0; i < payload_len; ++i) {
        printf("%02X", msg->Data[4 + i]);
        if (i + 1 < payload_len) {
            putchar(' ');
        }
    }
    printf(" rx=0x%08lX\n", msg->RxStatus);
}

static int install_pass_all_filter_404(Api* api, ULONG channel_id) {
    PASSTHRU_MSG_404 mask_msg;
    PASSTHRU_MSG_404 pattern_msg;
    PASSTHRU_MSG_404 flow_msg;
    ULONG filter_id = 0;
    PassThruStartMsgFilterFn404 filter_fn;
    long status;
    DWORD seh_code = 0;

    filter_fn = (PassThruStartMsgFilterFn404)GetProcAddress(api->module, "PassThruStartMsgFilter");
    if (!filter_fn) {
        logf("PassThruStartMsgFilter export missing; continuing without a filter");
        return 1;
    }

    memset(&mask_msg, 0, sizeof(mask_msg));
    memset(&pattern_msg, 0, sizeof(pattern_msg));
    memset(&flow_msg, 0, sizeof(flow_msg));
    mask_msg.ProtocolID = PROTOCOL_CAN;
    mask_msg.DataSize = 4;
    mask_msg.ExtraDataIndex = 4;
    mask_msg.Data[2] = 0xFF;
    mask_msg.Data[3] = 0xFF;
    pattern_msg.ProtocolID = PROTOCOL_CAN;
    pattern_msg.DataSize = 4;
    pattern_msg.ExtraDataIndex = 4;
    flow_msg.ProtocolID = PROTOCOL_CAN;
    flow_msg.DataSize = 4;
    flow_msg.ExtraDataIndex = 4;

    logf("Installing PASS_FILTER legacy ABI on channel=%lu mask=00 00 FF FF pattern=00 00 01 40", channel_id);
#if defined(_MSC_VER)
    __try {
        status = filter_fn(channel_id, PASS_FILTER, &mask_msg, &pattern_msg, &flow_msg, &filter_id);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        seh_code = GetExceptionCode();
        status = ERR_FAILED;
    }
#else
    status = filter_fn(channel_id, PASS_FILTER, &mask_msg, &pattern_msg, &flow_msg, &filter_id);
#endif
    if (seh_code != 0) {
        logf("PassThruStartMsgFilter legacy ABI raised exception 0x%08lX", seh_code);
        return 0;
    }
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruStartMsgFilter legacy ABI failed", status);
        return 0;
    }

    logf("Installed PASS_FILTER legacy ABI id=%lu for 11-bit traffic", filter_id);
    return 1;
}

static int can_rx_mode(Api* api, const RunConfig* cfg) {
    ULONG device_id = 0;
    ULONG channel_id = 0;
    ULONG resource_pins[2] = {6, 14};
    RESOURCE_STRUCT resource_struct;
    DWORD start_ticks;
    unsigned long long total_msgs = 0;
    long status;
    int exit_code = 1;

    memset(&resource_struct, 0, sizeof(resource_struct));
    resource_struct.Connector = J1962_CONNECTOR;
    resource_struct.NumOfResources = 2;
    resource_struct.ResourceListPtr = resource_pins;

    if (!try_open_with_fallback(api, cfg->open_name, &device_id)) {
        return 1;
    }

    status = api->PassThruConnect(device_id, PROTOCOL_CAN, cfg->connect_flags, cfg->baud, resource_struct, &channel_id);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruConnect failed", status);
        goto cleanup_device;
    }

    logf("PassThruConnect succeeded (channel_id=%lu baud=%lu flags=0x%08lX)", channel_id, cfg->baud, cfg->connect_flags);

    if (cfg->install_filter && !install_pass_all_filter(api, channel_id)) {
        goto cleanup_channel;
    }

    start_ticks = GetTickCount();
    while ((GetTickCount() - start_ticks) < cfg->duration_secs * 1000UL) {
        PASSTHRU_MSG msgs[16];
        unsigned char buffers[16][80];
        ULONG num_msgs = 16;
        ULONG i;
        DWORD seh_code = 0;

        memset(msgs, 0, sizeof(msgs));
        memset(buffers, 0, sizeof(buffers));
        for (i = 0; i < 16; ++i) {
            msgs[i].ProtocolID = PROTOCOL_CAN;
            msgs[i].DataBuffer = buffers[i];
            msgs[i].DataBufferSize = (ULONG)sizeof(buffers[i]);
        }

        status = safe_read_msgs(api, channel_id, msgs, &num_msgs, cfg->timeout_ms, &seh_code);
        if (seh_code != 0) {
            logf("PassThruReadMsgs raised exception 0x%08lX", seh_code);
            goto cleanup_channel;
        }
        if (status == ERR_BUFFER_EMPTY) {
            continue;
        }
        if (status != STATUS_NOERROR) {
            print_last_error(api, "PassThruReadMsgs failed", status);
            goto cleanup_channel;
        }

        total_msgs += num_msgs;
        for (i = 0; i < num_msgs; ++i) {
            print_can_frame(&msgs[i]);
        }
    }

    printf("Summary: received=%llu duration=%u baud=%lu flags=0x%08lX\n",
        total_msgs,
        cfg->duration_secs,
        cfg->baud,
        cfg->connect_flags);
    exit_code = 0;

cleanup_channel:
    status = api->PassThruDisconnect(channel_id);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruDisconnect failed", status);
    }
cleanup_device:
    status = api->PassThruClose(device_id);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruClose failed", status);
    }
    return exit_code;
}

static int can_rx_mode_legacy(Api* api, const RunConfig* cfg) {
    ULONG device_id = 0;
    ULONG channel_id = 0;
    DWORD start_ticks;
    unsigned long long total_msgs = 0;
    PassThruConnectFn404 connect_fn;
    PassThruReadMsgsFn404 read_fn;
    long status;
    int exit_code = 1;

    connect_fn = (PassThruConnectFn404)GetProcAddress(api->module, "PassThruConnect");
    read_fn = (PassThruReadMsgsFn404)GetProcAddress(api->module, "PassThruReadMsgs");
    if (!connect_fn || !read_fn) {
        logf("Legacy ABI exports missing");
        return 1;
    }

    if (!try_open_with_fallback(api, cfg->open_name, &device_id)) {
        return 1;
    }

#if defined(_MSC_VER)
    __try {
        status = connect_fn(device_id, PROTOCOL_CAN, cfg->connect_flags, cfg->baud, &channel_id);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        logf("PassThruConnect legacy ABI raised exception 0x%08lX", GetExceptionCode());
        goto cleanup_device;
    }
#else
    status = connect_fn(device_id, PROTOCOL_CAN, cfg->connect_flags, cfg->baud, &channel_id);
#endif
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruConnect legacy ABI failed", status);
        goto cleanup_device;
    }

    logf("PassThruConnect legacy ABI succeeded (channel_id=%lu baud=%lu flags=0x%08lX)", channel_id, cfg->baud, cfg->connect_flags);

    if (cfg->install_filter && !install_pass_all_filter_404(api, channel_id)) {
        goto cleanup_channel;
    }

    start_ticks = GetTickCount();
    while ((GetTickCount() - start_ticks) < cfg->duration_secs * 1000UL) {
        PASSTHRU_MSG_404 msgs[16];
        ULONG num_msgs = 16;
        ULONG i;
        DWORD seh_code = 0;

        memset(msgs, 0, sizeof(msgs));
        for (i = 0; i < 16; ++i) {
            msgs[i].ProtocolID = PROTOCOL_CAN;
        }

#if defined(_MSC_VER)
        __try {
            status = read_fn(channel_id, msgs, &num_msgs, cfg->timeout_ms);
        } __except (EXCEPTION_EXECUTE_HANDLER) {
            seh_code = GetExceptionCode();
            status = ERR_FAILED;
        }
#else
        status = read_fn(channel_id, msgs, &num_msgs, cfg->timeout_ms);
#endif
        if (seh_code != 0) {
            logf("PassThruReadMsgs legacy ABI raised exception 0x%08lX", seh_code);
            goto cleanup_channel;
        }
        if (status == ERR_BUFFER_EMPTY) {
            continue;
        }
        if (status != STATUS_NOERROR) {
            print_last_error(api, "PassThruReadMsgs legacy ABI failed", status);
            goto cleanup_channel;
        }

        total_msgs += num_msgs;
        for (i = 0; i < num_msgs; ++i) {
            print_can_frame_404(&msgs[i]);
        }
    }

    printf("Summary: abi=legacy received=%llu duration=%u baud=%lu flags=0x%08lX\n",
        total_msgs,
        cfg->duration_secs,
        cfg->baud,
        cfg->connect_flags);
    exit_code = 0;

cleanup_channel:
    status = api->PassThruDisconnect(channel_id);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruDisconnect failed", status);
    }
cleanup_device:
    status = api->PassThruClose(device_id);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruClose failed", status);
    }
    return exit_code;
}

/* ---------- v5 struct + old 6-param calling convention (hypothesis test) ---------- */
static int install_pass_all_filter_v5_6param(Api* api, ULONG channel_id) {
    unsigned char mask_bytes[4] = {0x00, 0x00, 0xFF, 0xFF};
    unsigned char pattern_bytes[4] = {0x00, 0x00, 0x01, 0x40};
    PASSTHRU_MSG mask_msg;
    PASSTHRU_MSG pattern_msg;
    ULONG filter_id = 0;
    long status;
    DWORD seh_code = 0;
    PassThruStartMsgFilterFn_V5_6Param filter_fn;

    filter_fn = (PassThruStartMsgFilterFn_V5_6Param)GetProcAddress(api->module, "PassThruStartMsgFilter");
    if (!filter_fn) {
        logf("PassThruStartMsgFilter export missing; continuing without a filter");
        return 1;
    }

    memset(&mask_msg, 0, sizeof(mask_msg));
    memset(&pattern_msg, 0, sizeof(pattern_msg));
    mask_msg.ProtocolID = PROTOCOL_CAN;
    mask_msg.DataBuffer = mask_bytes;
    mask_msg.DataBufferSize = 4;
    mask_msg.DataLength = 4;
    mask_msg.ExtraDataIndex = 4;
    pattern_msg.ProtocolID = PROTOCOL_CAN;
    pattern_msg.DataBuffer = pattern_bytes;
    pattern_msg.DataBufferSize = 4;
    pattern_msg.DataLength = 4;
    pattern_msg.ExtraDataIndex = 4;

    logf("Installing PASS_FILTER v5+6param on channel=%lu mask=00 00 FF FF pattern=00 00 01 40", channel_id);
#if defined(_MSC_VER)
    __try {
        status = filter_fn(channel_id, PASS_FILTER, &mask_msg, &pattern_msg, NULL, &filter_id);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        seh_code = GetExceptionCode();
        status = ERR_FAILED;
    }
#else
    status = filter_fn(channel_id, PASS_FILTER, &mask_msg, &pattern_msg, NULL, &filter_id);
#endif
    if (seh_code != 0) {
        logf("PassThruStartMsgFilter v5+6param raised exception 0x%08lX", seh_code);
        return 0;
    }
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruStartMsgFilter v5+6param failed", status);
        return 0;
    }

    logf("Installed PASS_FILTER v5+6param id=%lu", filter_id);
    return 1;
}

/* ---------- hybrid: v5 connect + v4.04 filter/read (Rust bridge strategy) ---------- */
static int can_rx_mode_hybrid(Api* api, const RunConfig* cfg) {
    ULONG device_id = 0;
    ULONG channel_id = 0;
    ULONG resource_pins[2] = {6, 14};
    RESOURCE_STRUCT resource_struct;
    DWORD start_ticks;
    unsigned long long total_msgs = 0;
    PassThruReadMsgsFn404 read_fn;
    long status;
    int exit_code = 1;

    read_fn = (PassThruReadMsgsFn404)GetProcAddress(api->module, "PassThruReadMsgs");
    if (!read_fn) {
        logf("PassThruReadMsgs export missing");
        return 1;
    }

    memset(&resource_struct, 0, sizeof(resource_struct));
    resource_struct.Connector = J1962_CONNECTOR;
    resource_struct.NumOfResources = 2;
    resource_struct.ResourceListPtr = resource_pins;

    if (!try_open_with_fallback(api, cfg->open_name, &device_id)) {
        return 1;
    }

    /* v5 connect (6-param with RESOURCE_STRUCT) */
    status = api->PassThruConnect(device_id, PROTOCOL_CAN, cfg->connect_flags, cfg->baud, resource_struct, &channel_id);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruConnect (v5) failed", status);
        goto cleanup_device;
    }
    logf("PassThruConnect v5 succeeded (channel_id=%lu baud=%lu flags=0x%08lX)", channel_id, cfg->baud, cfg->connect_flags);

    /* v4.04 filter (6-param, inline PASSTHRU_MSG_404) */
    if (cfg->install_filter && !install_pass_all_filter_404(api, channel_id)) {
        goto cleanup_channel;
    }

    /* v4.04 read (inline PASSTHRU_MSG_404) */
    start_ticks = GetTickCount();
    while ((GetTickCount() - start_ticks) < cfg->duration_secs * 1000UL) {
        PASSTHRU_MSG_404 msgs[16];
        ULONG num_msgs = 16;
        ULONG i;
        DWORD seh_code = 0;

        memset(msgs, 0, sizeof(msgs));
        for (i = 0; i < 16; ++i) {
            msgs[i].ProtocolID = PROTOCOL_CAN;
        }

#if defined(_MSC_VER)
        __try {
            status = read_fn(channel_id, msgs, &num_msgs, cfg->timeout_ms);
        } __except (EXCEPTION_EXECUTE_HANDLER) {
            seh_code = GetExceptionCode();
            status = ERR_FAILED;
        }
#else
        status = read_fn(channel_id, msgs, &num_msgs, cfg->timeout_ms);
#endif
        if (seh_code != 0) {
            logf("PassThruReadMsgs hybrid raised exception 0x%08lX", seh_code);
            goto cleanup_channel;
        }
        if (status == ERR_BUFFER_EMPTY) {
            continue;
        }
        if (status != STATUS_NOERROR) {
            print_last_error(api, "PassThruReadMsgs hybrid failed", status);
            goto cleanup_channel;
        }

        total_msgs += num_msgs;
        for (i = 0; i < num_msgs; ++i) {
            print_can_frame_404(&msgs[i]);
        }
    }

    printf("Summary: abi=hybrid(v5-connect+v404-filter/read) received=%llu duration=%u baud=%lu flags=0x%08lX\n",
        total_msgs, cfg->duration_secs, cfg->baud, cfg->connect_flags);
    exit_code = 0;

cleanup_channel:
    status = api->PassThruDisconnect(channel_id);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruDisconnect failed", status);
    }
cleanup_device:
    status = api->PassThruClose(device_id);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruClose failed", status);
    }
    return exit_code;
}

/* ---------- v5-6param: v5 connect + 6-param filter (v5 struct) + v5 read ---------- */
static int can_rx_mode_v5_6param(Api* api, const RunConfig* cfg) {
    ULONG device_id = 0;
    ULONG channel_id = 0;
    ULONG resource_pins[2] = {6, 14};
    RESOURCE_STRUCT resource_struct;
    DWORD start_ticks;
    unsigned long long total_msgs = 0;
    long status;
    int exit_code = 1;

    memset(&resource_struct, 0, sizeof(resource_struct));
    resource_struct.Connector = J1962_CONNECTOR;
    resource_struct.NumOfResources = 2;
    resource_struct.ResourceListPtr = resource_pins;

    if (!try_open_with_fallback(api, cfg->open_name, &device_id)) {
        return 1;
    }

    /* v5 connect */
    status = api->PassThruConnect(device_id, PROTOCOL_CAN, cfg->connect_flags, cfg->baud, resource_struct, &channel_id);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruConnect (v5) failed", status);
        goto cleanup_device;
    }
    logf("PassThruConnect v5 succeeded (channel_id=%lu baud=%lu flags=0x%08lX)", channel_id, cfg->baud, cfg->connect_flags);

    /* 6-param filter with v5 struct (hypothesis: DLL kept the old param count) */
    if (cfg->install_filter && !install_pass_all_filter_v5_6param(api, channel_id)) {
        goto cleanup_channel;
    }

    /* v5 read (pointer-based PASSTHRU_MSG) */
    start_ticks = GetTickCount();
    while ((GetTickCount() - start_ticks) < cfg->duration_secs * 1000UL) {
        PASSTHRU_MSG msgs[16];
        unsigned char buffers[16][80];
        ULONG num_msgs = 16;
        ULONG i;
        DWORD seh_code = 0;

        memset(msgs, 0, sizeof(msgs));
        memset(buffers, 0, sizeof(buffers));
        for (i = 0; i < 16; ++i) {
            msgs[i].ProtocolID = PROTOCOL_CAN;
            msgs[i].DataBuffer = buffers[i];
            msgs[i].DataBufferSize = (ULONG)sizeof(buffers[i]);
        }

        status = safe_read_msgs(api, channel_id, msgs, &num_msgs, cfg->timeout_ms, &seh_code);
        if (seh_code != 0) {
            logf("PassThruReadMsgs v5 raised exception 0x%08lX", seh_code);
            goto cleanup_channel;
        }
        if (status == ERR_BUFFER_EMPTY) {
            continue;
        }
        if (status != STATUS_NOERROR) {
            print_last_error(api, "PassThruReadMsgs v5 failed", status);
            goto cleanup_channel;
        }

        total_msgs += num_msgs;
        for (i = 0; i < num_msgs; ++i) {
            print_can_frame(&msgs[i]);
        }
    }

    printf("Summary: abi=v5-6param received=%llu duration=%u baud=%lu flags=0x%08lX\n",
        total_msgs, cfg->duration_secs, cfg->baud, cfg->connect_flags);
    exit_code = 0;

cleanup_channel:
    status = api->PassThruDisconnect(channel_id);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruDisconnect failed", status);
    }
cleanup_device:
    status = api->PassThruClose(device_id);
    if (status != STATUS_NOERROR) {
        print_last_error(api, "PassThruClose failed", status);
    }
    return exit_code;
}

int main(int argc, char** argv) {
    const char* mode;
    Api api;
    RunConfig cfg;
    int i;

    if (argc < 2) {
        usage(argv[0]);
        return 2;
    }

    mode = argv[1];
    cfg.dll_path = "C:\\Program Files (x86)\\PEAK-System\\PCAN-PassThru API\\05.00\\x64\\PEAKPT64.dll";
    cfg.open_name = NULL;
    cfg.baud = 500000UL;
    cfg.connect_flags = 0UL;
    cfg.timeout_ms = 250UL;
    cfg.duration_secs = 10U;
    cfg.install_filter = 1;

    for (i = 2; i < argc; ++i) {
        if (strcmp(argv[i], "--dll") == 0 && i + 1 < argc) {
            cfg.dll_path = argv[++i];
        } else if (strcmp(argv[i], "--open-name") == 0 && i + 1 < argc) {
            cfg.open_name = argv[++i];
        } else if (strcmp(argv[i], "--baud") == 0 && i + 1 < argc) {
            if (!parse_ulong_arg(argv[++i], &cfg.baud)) {
                logf("Invalid --baud value");
                return 2;
            }
        } else if (strcmp(argv[i], "--duration") == 0 && i + 1 < argc) {
            ULONG value = 0;
            if (!parse_ulong_arg(argv[++i], &value)) {
                logf("Invalid --duration value");
                return 2;
            }
            cfg.duration_secs = (unsigned)value;
        } else if (strcmp(argv[i], "--timeout-ms") == 0 && i + 1 < argc) {
            if (!parse_ulong_arg(argv[++i], &cfg.timeout_ms)) {
                logf("Invalid --timeout-ms value");
                return 2;
            }
        } else if (strcmp(argv[i], "--connect-flags") == 0 && i + 1 < argc) {
            if (!parse_ulong_arg(argv[++i], &cfg.connect_flags)) {
                logf("Invalid --connect-flags value");
                return 2;
            }
        } else if (strcmp(argv[i], "--skip-filter") == 0) {
            cfg.install_filter = 0;
        } else {
            usage(argv[0]);
            return 2;
        }
    }

    if (!load_api(&api, cfg.dll_path)) {
        return 1;
    }

    if (strcmp(mode, "list") == 0) {
        i = list_mode(&api);
    } else if (strcmp(mode, "can-rx") == 0) {
        i = can_rx_mode(&api, &cfg);
    } else if (strcmp(mode, "can-rx-legacy") == 0) {
        i = can_rx_mode_legacy(&api, &cfg);
    } else if (strcmp(mode, "can-rx-hybrid") == 0) {
        i = can_rx_mode_hybrid(&api, &cfg);
    } else if (strcmp(mode, "can-rx-v5-6param") == 0) {
        i = can_rx_mode_v5_6param(&api, &cfg);
    } else {
        usage(argv[0]);
        i = 2;
    }

    unload_api(&api);
    return i;
}
