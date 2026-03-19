use libloading::{Library, Symbol};
use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_ulong, c_void};
use std::sync::Mutex;
use winreg::enums::*;
use winreg::RegKey;

// J2534 Protocol IDs
// Note: Only CAN protocol is supported. Other protocols (ISO15765, ISO9141, etc.)
// are optional in the J2534 spec, so adapter support is inconsistent.
#[allow(dead_code)]
pub const PROTOCOL_J1850VPW: u32 = 1;
#[allow(dead_code)]
pub const PROTOCOL_J1850PWM: u32 = 2;
#[allow(dead_code)]
pub const PROTOCOL_ISO9141: u32 = 3;
#[allow(dead_code)]
pub const PROTOCOL_ISO14230: u32 = 4;
pub const PROTOCOL_CAN: u32 = 5;
#[allow(dead_code)]
pub const PROTOCOL_ISO15765: u32 = 6;
#[allow(dead_code)]
pub const PROTOCOL_SCI_A_ENGINE: u32 = 7;
#[allow(dead_code)]
pub const PROTOCOL_SCI_A_TRANS: u32 = 8;
#[allow(dead_code)]
pub const PROTOCOL_SCI_B_ENGINE: u32 = 9;
#[allow(dead_code)]
pub const PROTOCOL_SCI_B_TRANS: u32 = 10;

// J2534 Connect Flags
pub const CAN_29BIT_ID: u32 = 0x100;
#[allow(dead_code)]
pub const ISO9141_NO_CHECKSUM: u32 = 0x200;
#[allow(dead_code)]
pub const CAN_ID_BOTH: u32 = 0x800;
pub const CAN_MIXED_CAPTURE_FLAGS: u32 = CAN_ID_BOTH | CAN_29BIT_ID;
#[allow(dead_code)]
pub const ISO9141_K_LINE_ONLY: u32 = 0x1000;

// J2534 TxFlags
#[allow(dead_code)]
pub const ISO15765_FRAME_PAD: u32 = 0x0040;
#[allow(dead_code)]
pub const ISO15765_ADDR_TYPE: u32 = 0x0080;

// J2534 Filter Types
pub const PASS_FILTER: u32 = 1;
#[allow(dead_code)]
pub const BLOCK_FILTER: u32 = 2;
#[allow(dead_code)]
pub const FLOW_CONTROL_FILTER: u32 = 3;

// J2534 IOCTL IDs
pub const GET_CONFIG: u32 = 0x01;
pub const SET_CONFIG: u32 = 0x02;
pub const READ_VBATT: u32 = 0x03;
#[allow(dead_code)]
pub const FIVE_BAUD_INIT: u32 = 0x04;
#[allow(dead_code)]
pub const FAST_INIT: u32 = 0x05;
pub const CLEAR_TX_BUFFER: u32 = 0x07;
pub const CLEAR_RX_BUFFER: u32 = 0x08;
pub const CLEAR_PERIODIC_MSGS: u32 = 0x09;
pub const CLEAR_MSG_FILTERS: u32 = 0x0A;
#[allow(dead_code)]
pub const CLEAR_FUNCT_MSG_LOOKUP_TABLE: u32 = 0x0B;
#[allow(dead_code)]
pub const ADD_TO_FUNCT_MSG_LOOKUP_TABLE: u32 = 0x0C;
#[allow(dead_code)]
pub const DELETE_FROM_FUNCT_MSG_LOOKUP_TABLE: u32 = 0x0D;
pub const READ_PROG_VOLTAGE: u32 = 0x0E;

// J2534 Config Parameter IDs
pub const DATA_RATE: u32 = 0x01;
pub const LOOPBACK: u32 = 0x03;
#[allow(dead_code)]
pub const NODE_ADDRESS: u32 = 0x04;
#[allow(dead_code)]
pub const NETWORK_LINE: u32 = 0x05;
#[allow(dead_code)]
pub const P1_MIN: u32 = 0x06;
#[allow(dead_code)]
pub const P1_MAX: u32 = 0x07;
#[allow(dead_code)]
pub const P2_MIN: u32 = 0x08;
#[allow(dead_code)]
pub const P2_MAX: u32 = 0x09;
#[allow(dead_code)]
pub const P3_MIN: u32 = 0x0A;
#[allow(dead_code)]
pub const P3_MAX: u32 = 0x0B;
#[allow(dead_code)]
pub const P4_MIN: u32 = 0x0C;
#[allow(dead_code)]
pub const P4_MAX: u32 = 0x0D;
#[allow(dead_code)]
pub const W1: u32 = 0x0E;
#[allow(dead_code)]
pub const W2: u32 = 0x0F;
#[allow(dead_code)]
pub const W3: u32 = 0x10;
#[allow(dead_code)]
pub const W4: u32 = 0x11;
#[allow(dead_code)]
pub const W5: u32 = 0x12;
#[allow(dead_code)]
pub const TIDLE: u32 = 0x13;
#[allow(dead_code)]
pub const TINIL: u32 = 0x14;
#[allow(dead_code)]
pub const TWUP: u32 = 0x15;
#[allow(dead_code)]
pub const PARITY: u32 = 0x16;
#[allow(dead_code)]
pub const BIT_SAMPLE_POINT: u32 = 0x17;
#[allow(dead_code)]
pub const SYNC_JUMP_WIDTH: u32 = 0x18;
#[allow(dead_code)]
pub const W0: u32 = 0x19;
#[allow(dead_code)]
pub const T1_MAX: u32 = 0x1A;
#[allow(dead_code)]
pub const T2_MAX: u32 = 0x1B;
#[allow(dead_code)]
pub const T4_MAX: u32 = 0x1C;
#[allow(dead_code)]
pub const T5_MAX: u32 = 0x1D;
#[allow(dead_code)]
pub const ISO15765_BS: u32 = 0x1E;
#[allow(dead_code)]
pub const ISO15765_STMIN: u32 = 0x1F;
#[allow(dead_code)]
pub const DATA_BITS: u32 = 0x20;
#[allow(dead_code)]
pub const FIVE_BAUD_MOD: u32 = 0x21;
#[allow(dead_code)]
pub const BS_TX: u32 = 0x22;
#[allow(dead_code)]
pub const STMIN_TX: u32 = 0x23;
#[allow(dead_code)]
pub const T3_MAX: u32 = 0x24;
#[allow(dead_code)]
pub const ISO15765_WFT_MAX: u32 = 0x25;

// J2534 Error codes
pub const STATUS_NOERROR: i32 = 0x00;
pub const ERR_NOT_SUPPORTED: i32 = 0x01;
pub const ERR_INVALID_CHANNEL_ID: i32 = 0x02;
pub const ERR_INVALID_PROTOCOL_ID: i32 = 0x03;
pub const ERR_NULL_PARAMETER: i32 = 0x04;
pub const ERR_INVALID_IOCTL_VALUE: i32 = 0x05;
pub const ERR_INVALID_FLAGS: i32 = 0x06;
pub const ERR_FAILED: i32 = 0x07;
pub const ERR_DEVICE_NOT_CONNECTED: i32 = 0x08;
pub const ERR_TIMEOUT: i32 = 0x09;
pub const ERR_INVALID_MSG: i32 = 0x0A;
pub const ERR_INVALID_TIME_INTERVAL: i32 = 0x0B;
pub const ERR_EXCEEDED_LIMIT: i32 = 0x0C;
pub const ERR_INVALID_MSG_ID: i32 = 0x0D;
pub const ERR_DEVICE_IN_USE: i32 = 0x0E;
pub const ERR_INVALID_IOCTL_ID: i32 = 0x0F;
pub const ERR_BUFFER_EMPTY: i32 = 0x10;
pub const ERR_BUFFER_FULL: i32 = 0x11;
pub const ERR_BUFFER_OVERFLOW: i32 = 0x12;
pub const ERR_PIN_INVALID: i32 = 0x13;
pub const ERR_CHANNEL_IN_USE: i32 = 0x14;
pub const ERR_MSG_PROTOCOL_ID: i32 = 0x15;
pub const ERR_INVALID_FILTER_ID: i32 = 0x16;
pub const ERR_NO_FLOW_CONTROL: i32 = 0x17;
pub const ERR_NOT_UNIQUE: i32 = 0x18;
pub const ERR_INVALID_BAUDRATE: i32 = 0x19;
pub const ERR_INVALID_DEVICE_ID: i32 = 0x1A;

// RX Status flags
pub const RX_CAN_29BIT_ID: u32 = 0x100;
pub const TX_MSG_TYPE: u32 = 0x01;

fn get_last_error_message(library: &Library) -> Option<String> {
    unsafe {
        let get_last_error_fn: Symbol<PassThruGetLastErrorFn> =
            library.get(b"PassThruGetLastError\0").ok()?;
        let mut buffer = [0i8; 512];
        let result = get_last_error_fn(buffer.as_mut_ptr());
        if result != STATUS_NOERROR {
            return None;
        }
        let bytes: Vec<u8> = buffer
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as u8)
            .collect();
        let msg = String::from_utf8_lossy(&bytes).trim().to_string();
        if msg.is_empty() {
            None
        } else {
            Some(msg)
        }
    }
}

/// Get error code description
pub fn error_code_to_string(code: i32) -> &'static str {
    match code {
        STATUS_NOERROR => "STATUS_NOERROR",
        ERR_NOT_SUPPORTED => "ERR_NOT_SUPPORTED",
        ERR_INVALID_CHANNEL_ID => "ERR_INVALID_CHANNEL_ID",
        ERR_INVALID_PROTOCOL_ID => "ERR_INVALID_PROTOCOL_ID",
        ERR_NULL_PARAMETER => "ERR_NULL_PARAMETER",
        ERR_INVALID_IOCTL_VALUE => "ERR_INVALID_IOCTL_VALUE",
        ERR_INVALID_FLAGS => "ERR_INVALID_FLAGS",
        ERR_FAILED => "ERR_FAILED",
        ERR_DEVICE_NOT_CONNECTED => "ERR_DEVICE_NOT_CONNECTED",
        ERR_TIMEOUT => "ERR_TIMEOUT",
        ERR_INVALID_MSG => "ERR_INVALID_MSG",
        ERR_INVALID_TIME_INTERVAL => "ERR_INVALID_TIME_INTERVAL",
        ERR_EXCEEDED_LIMIT => "ERR_EXCEEDED_LIMIT",
        ERR_INVALID_MSG_ID => "ERR_INVALID_MSG_ID",
        ERR_DEVICE_IN_USE => "ERR_DEVICE_IN_USE",
        ERR_INVALID_IOCTL_ID => "ERR_INVALID_IOCTL_ID",
        ERR_BUFFER_EMPTY => "ERR_BUFFER_EMPTY",
        ERR_BUFFER_FULL => "ERR_BUFFER_FULL",
        ERR_BUFFER_OVERFLOW => "ERR_BUFFER_OVERFLOW",
        ERR_PIN_INVALID => "ERR_PIN_INVALID",
        ERR_CHANNEL_IN_USE => "ERR_CHANNEL_IN_USE",
        ERR_MSG_PROTOCOL_ID => "ERR_MSG_PROTOCOL_ID",
        ERR_INVALID_FILTER_ID => "ERR_INVALID_FILTER_ID",
        ERR_NO_FLOW_CONTROL => "ERR_NO_FLOW_CONTROL",
        ERR_NOT_UNIQUE => "ERR_NOT_UNIQUE",
        ERR_INVALID_BAUDRATE => "ERR_INVALID_BAUDRATE",
        ERR_INVALID_DEVICE_ID => "ERR_INVALID_DEVICE_ID",
        _ => "UNKNOWN_ERROR",
    }
}

/// Get protocol name from ID
#[allow(dead_code)]
pub fn protocol_id_to_string(id: u32) -> &'static str {
    match id {
        PROTOCOL_J1850VPW => "J1850VPW",
        PROTOCOL_J1850PWM => "J1850PWM",
        PROTOCOL_ISO9141 => "ISO9141",
        PROTOCOL_ISO14230 => "ISO14230",
        PROTOCOL_CAN => "CAN",
        PROTOCOL_ISO15765 => "ISO15765",
        PROTOCOL_SCI_A_ENGINE => "SCI_A_ENGINE",
        PROTOCOL_SCI_A_TRANS => "SCI_A_TRANS",
        PROTOCOL_SCI_B_ENGINE => "SCI_B_ENGINE",
        PROTOCOL_SCI_B_TRANS => "SCI_B_TRANS",
        _ => "UNKNOWN",
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct PassThruMsg {
    pub protocol_id: u32,
    pub rx_status: u32,
    pub tx_flags: u32,
    pub timestamp: u32,
    pub data_size: u32,
    pub extra_data_index: u32,
    pub data: [u8; 4128],
}

impl Default for PassThruMsg {
    fn default() -> Self {
        Self {
            protocol_id: 0,
            rx_status: 0,
            tx_flags: 0,
            timestamp: 0,
            data_size: 0,
            extra_data_index: 0,
            data: [0u8; 4128],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534Device {
    pub name: String,
    pub vendor: String,
    pub dll_path: String,
    pub can_iso15765: bool,
    pub can_iso11898: bool,
    pub compatible: bool,
    pub bitness: u8,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub api_version: String,
    pub supported_protocols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CANMessage {
    pub timestamp_us: u64,
    pub arb_id: u32,
    pub extended: bool,
    pub data: Vec<u8>,
    pub raw_arb_id: u32,
    pub rx_status: u32,
    pub data_size: u32,
    /// J2534 protocol ID (5=CAN, 3=ISO9141, 4=ISO14230, etc.)
    #[serde(default = "default_protocol_can")]
    pub protocol_id: u32,
}

/// Bridge-internal result of a full K-Line init sequence.
pub struct KlineInitResult {
    pub init_method: String,
    pub detected_protocol: String,
    pub keyword_bytes: Vec<u8>,
    pub cc_received: bool,
    pub init_response: Vec<CANMessage>,
}

fn default_protocol_can() -> u32 {
    PROTOCOL_CAN
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534Progress {
    pub step: String,
    pub status: String, // "pending" | "in_progress" | "success" | "error"
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534VersionInfo {
    pub firmware_version: String,
    pub dll_version: String,
    pub api_version: String,
}

#[derive(Debug, Clone)]
pub struct RawIoResult {
    pub result: i32,
    pub num_msgs: u32,
}

fn passthru_msg_to_can_message(msg: &PassThruMsg) -> CANMessage {
    let is_can = msg.protocol_id == PROTOCOL_CAN || msg.protocol_id == PROTOCOL_ISO15765;
    if is_can && msg.data_size >= 4 {
        let arb_id = u32::from_be_bytes([msg.data[0], msg.data[1], msg.data[2], msg.data[3]]);
        let data_len = (msg.data_size - 4) as usize;
        let data = msg.data[4..4 + data_len].to_vec();
        CANMessage {
            timestamp_us: msg.timestamp as u64,
            arb_id,
            extended: (msg.rx_status & RX_CAN_29BIT_ID) != 0,
            data,
            raw_arb_id: arb_id,
            rx_status: msg.rx_status,
            data_size: msg.data_size,
            protocol_id: msg.protocol_id,
        }
    } else {
        let data = msg.data[..msg.data_size as usize].to_vec();
        CANMessage {
            timestamp_us: msg.timestamp as u64,
            arb_id: 0,
            extended: false,
            data,
            raw_arb_id: 0,
            rx_status: msg.rx_status,
            data_size: msg.data_size,
            protocol_id: msg.protocol_id,
        }
    }
}

/// SCONFIG structure for GET_CONFIG/SET_CONFIG
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SConfig {
    pub parameter: u32,
    pub value: u32,
}

/// SCONFIG_LIST structure
#[repr(C)]
#[derive(Debug)]
pub struct SConfigList {
    pub num_of_params: u32,
    pub config_ptr: *mut SConfig,
}

/// SBYTE_ARRAY structure used by certain IOCTLs such as FIVE_BAUD_INIT.
#[repr(C)]
#[derive(Debug)]
pub struct SByteArray {
    pub num_of_bytes: u32,
    pub byte_ptr: *mut u8,
}

/// Tracks a recently sent message for TX echo filtering
#[derive(Clone)]
pub struct SentMessage {
    pub arb_id: u32,
    pub data: Vec<u8>,
    pub timestamp: std::time::Instant,
}

fn get_dll_bitness(dll_path: &str) -> Option<u8> {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    let mut file = File::open(dll_path).ok()?;

    let mut dos_header = [0u8; 64];
    file.read_exact(&mut dos_header).ok()?;

    if &dos_header[0..2] != b"MZ" {
        return None;
    }

    let pe_offset = u32::from_le_bytes([
        dos_header[0x3C],
        dos_header[0x3D],
        dos_header[0x3E],
        dos_header[0x3F],
    ]);

    file.seek(SeekFrom::Start(pe_offset as u64)).ok()?;

    let mut pe_header = [0u8; 6];
    file.read_exact(&mut pe_header).ok()?;

    if &pe_header[0..4] != b"PE\0\0" {
        return None;
    }

    let machine = u16::from_le_bytes([pe_header[4], pe_header[5]]);
    match machine {
        0x014c => Some(32),
        0x8664 | 0xAA64 => Some(64),
        _ => None,
    }
}

/// Detect J2534 API version by probing for v5.0 symbol exports.
/// Results are cached persistently keyed by DLL path + modification time
/// to avoid loading DLLs on every enumeration.
pub fn detect_api_version(dll_path: &str) -> String {
    let modified = std::fs::metadata(dll_path)
        .and_then(|m| m.modified())
        .ok();
    let cache_key = match &modified {
        Some(t) => format!(
            "{}|{}",
            dll_path,
            t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
        ),
        None => dll_path.to_string(),
    };

    if let Some(cached) = api_version_cache_read(&cache_key) {
        return cached;
    }

    let version = detect_api_version_uncached(dll_path);
    api_version_cache_write(&cache_key, &version);
    version
}

fn detect_api_version_uncached(dll_path: &str) -> String {
    let lib = match unsafe { libloading::Library::new(dll_path) } {
        Ok(lib) => lib,
        Err(_) => return "04.04".to_string(),
    };
    let has_scan: bool =
        unsafe { lib.get::<*const ()>(b"PassThruScanForDevices\0").is_ok() };
    if has_scan {
        "05.00".to_string()
    } else {
        "04.04".to_string()
    }
}

fn api_version_cache_path() -> Option<std::path::PathBuf> {
    std::env::var("LOCALAPPDATA")
        .ok()
        .map(|dir| std::path::PathBuf::from(dir).join("j2534-bridge").join("api_version_cache.json"))
}

fn api_version_cache_read(key: &str) -> Option<String> {
    let path = api_version_cache_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    let map: std::collections::HashMap<String, String> = serde_json::from_str(&content).ok()?;
    map.get(key).cloned()
}

fn api_version_cache_write(key: &str, value: &str) {
    let Some(path) = api_version_cache_path() else { return };
    let mut map: std::collections::HashMap<String, String> = path
        .exists()
        .then(|| std::fs::read_to_string(&path).ok())
        .flatten()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();
    map.insert(key.to_string(), value.to_string());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&map).unwrap_or_default());
}

fn clean_device_name(name: &str) -> String {
    let mut clean = name.to_string();
    for marker in [
        "(64-bit)",
        "(32-bit)",
        "(64 bit)",
        "(32 bit)",
        "(incompatible)",
        "(compatible)",
        "(x64)",
        "(x86)",
        "(missing)",
    ] {
        clean = clean.replace(marker, "");
    }
    clean.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn enumerate_devices() -> Vec<J2534Device> {
    let mut devices = Vec::new();

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    // Check both 32-bit and 64-bit registry locations, and both v4.04 and v5.00
    let registry_paths = [
        (
            "SOFTWARE\\PassThruSupport.04.04",
            KEY_READ | KEY_WOW64_64KEY,
        ),
        (
            "SOFTWARE\\PassThruSupport.04.04",
            KEY_READ | KEY_WOW64_32KEY,
        ),
        (
            "SOFTWARE\\PassThruSupport.05.00",
            KEY_READ | KEY_WOW64_64KEY,
        ),
        (
            "SOFTWARE\\PassThruSupport.05.00",
            KEY_READ | KEY_WOW64_32KEY,
        ),
    ];

    for (path, flags) in &registry_paths {
        let is_v5_registry = path.contains("05.00");
        if let Ok(passthru_key) = hklm.open_subkey_with_flags(path, *flags) {
            for device_name in passthru_key.enum_keys().filter_map(|k| k.ok()) {
                if let Ok(device_key) = passthru_key.open_subkey_with_flags(&device_name, *flags) {
                    let name: String = device_key.get_value("Name").unwrap_or(device_name.clone());
                    let vendor: String = device_key.get_value("Vendor").unwrap_or_default();
                    let dll_path: String =
                        device_key.get_value("FunctionLibrary").unwrap_or_default();
                    // v5.0 registry entries typically omit protocol flags;
                    // default to true since v5.0 devices support CAN by spec
                    let default_protocol: u32 = if is_v5_registry { 1 } else { 0 };
                    let can_iso15765: u32 =
                        device_key.get_value("ISO15765").unwrap_or(default_protocol);
                    let can_iso11898: u32 =
                        device_key.get_value("CAN").unwrap_or(default_protocol);

                    // Collect all supported protocols from registry
                    let mut supported_protocols = Vec::new();
                    let protocol_keys: &[(&str, &str)] = &[
                        ("CAN", "CAN"),
                        ("ISO15765", "ISO-TP"),
                        ("ISO9141", "ISO 9141"),
                        ("ISO14230", "ISO 14230"),
                        ("J1850PWM", "J1850 PWM"),
                        ("J1850VPW", "J1850 VPW"),
                        ("CAN_FD_PS", "CAN-FD"),
                        ("ISO15765_FD_PS", "ISO-TP FD"),
                        ("SW_CAN_PS", "SW-CAN"),
                        ("TP2_0_PS", "TP 2.0"),
                    ];
                    for &(reg_key, display_name) in protocol_keys {
                        let val: u32 = device_key.get_value(reg_key).unwrap_or(
                            if is_v5_registry && (reg_key == "CAN" || reg_key == "ISO15765") { 1 } else { 0 }
                        );
                        if val != 0 {
                            supported_protocols.push(display_name.to_string());
                        }
                    }

                    if dll_path.is_empty() {
                        continue;
                    }

                    // Check if we already have this device (from the other registry view)
                    if devices.iter().any(|d: &J2534Device| d.dll_path == dll_path) {
                        continue;
                    }

                    let bitness_result = get_dll_bitness(&dll_path);
                    let (available, unavailable_reason, api_version, bitness) =
                        match bitness_result {
                            None => (
                                false,
                                Some("DLL not found".to_string()),
                                if is_v5_registry {
                                    "05.00".to_string()
                                } else {
                                    "04.04".to_string()
                                },
                                if cfg!(target_pointer_width = "64") { 64 } else { 32 },
                            ),
                            Some(bits) => (
                                true,
                                None,
                                detect_api_version(&dll_path),
                                bits,
                            ),
                        };
                    let mut clean_name = clean_device_name(&name);
                    // Disambiguate v5.0 entries from their v4.04 counterparts
                    if is_v5_registry
                        && devices.iter().any(|d: &J2534Device| d.name == clean_name)
                    {
                        clean_name = format!("{} (v5.0)", clean_name);
                    }

                    devices.push(J2534Device {
                        name: clean_name,
                        vendor,
                        dll_path,
                        can_iso15765: can_iso15765 != 0,
                        can_iso11898: can_iso11898 != 0,
                        compatible: true,
                        bitness,
                        available,
                        unavailable_reason,
                        api_version,
                        supported_protocols,
                    });
                }
            }
        }
    }

    devices
}

// Use "system" calling convention which is stdcall on Windows and C on other platforms
type PassThruOpenFn = unsafe extern "system" fn(*const c_void, *mut c_ulong) -> i32;
type PassThruCloseFn = unsafe extern "system" fn(c_ulong) -> i32;
type PassThruConnectFn =
    unsafe extern "system" fn(c_ulong, c_ulong, c_ulong, c_ulong, *mut c_ulong) -> i32;
type PassThruDisconnectFn = unsafe extern "system" fn(c_ulong) -> i32;
type PassThruReadMsgsFn =
    unsafe extern "system" fn(c_ulong, *mut PassThruMsg, *mut c_ulong, c_ulong) -> i32;
type PassThruWriteMsgsFn =
    unsafe extern "system" fn(c_ulong, *mut PassThruMsg, *mut c_ulong, c_ulong) -> i32;
type PassThruStartMsgFilterFn = unsafe extern "system" fn(
    c_ulong,
    c_ulong,
    *const PassThruMsg,
    *const PassThruMsg,
    *const PassThruMsg,
    *mut c_ulong,
) -> i32;
type PassThruStopMsgFilterFn = unsafe extern "system" fn(c_ulong, c_ulong) -> i32;
type PassThruStartPeriodicMsgFn =
    unsafe extern "system" fn(c_ulong, *const PassThruMsg, *mut c_ulong, c_ulong) -> i32;
type PassThruStopPeriodicMsgFn = unsafe extern "system" fn(c_ulong, c_ulong) -> i32;
type PassThruIoctlFn = unsafe extern "system" fn(c_ulong, c_ulong, *mut c_void, *mut c_void) -> i32;
type PassThruReadVersionFn =
    unsafe extern "system" fn(c_ulong, *mut c_char, *mut c_char, *mut c_char) -> i32;
type PassThruGetLastErrorFn = unsafe extern "system" fn(*mut c_char) -> i32;
#[allow(dead_code)]
type PassThruSetProgrammingVoltageFn = unsafe extern "system" fn(c_ulong, c_ulong, c_ulong) -> i32;

pub struct J2534Connection {
    library: Library,
    device_id: u32,
    channel_id: u32,
    /// Optional second channel for 29-bit CAN IDs (used when CAN_ID_BOTH is
    /// not supported by the adapter — we fall back to dual channels).
    channel_id_29bit: Option<u32>,
    protocol_id: u32,
    /// Recently sent messages for filtering TX echoes (driver workaround)
    sent_messages: Mutex<Vec<SentMessage>>,
}

impl J2534Connection {
    pub fn open(
        dll_path: &str,
        protocol_id: u32,
        baud_rate: u32,
        connect_flags: u32,
        progress_callback: impl Fn(J2534Progress),
    ) -> Result<Self, String> {
        // Load the DLL
        progress_callback(J2534Progress {
            step: "load_dll".to_string(),
            status: "in_progress".to_string(),
            message: Some(dll_path.to_string()),
        });

        let library =
            unsafe { Library::new(dll_path) }.map_err(|e| format!("ERR_J2534_DLL_LOAD: {}", e))?;

        progress_callback(J2534Progress {
            step: "load_dll".to_string(),
            status: "success".to_string(),
            message: None,
        });

        // Open the device
        progress_callback(J2534Progress {
            step: "open_device".to_string(),
            status: "in_progress".to_string(),
            message: None,
        });

        let mut device_id: c_ulong = 0;
        unsafe {
            let open_fn: Symbol<PassThruOpenFn> = library
                .get(b"PassThruOpen\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruOpen - {}", e))?;

            let result = open_fn(std::ptr::null(), &mut device_id);
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_OPEN_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        progress_callback(J2534Progress {
            step: "open_device".to_string(),
            status: "success".to_string(),
            message: Some(format!("Device ID: {}", device_id)),
        });

        // Connect to CAN channel
        progress_callback(J2534Progress {
            step: "connect_channel".to_string(),
            status: "in_progress".to_string(),
            message: Some(format!("Baud rate: {} bps", baud_rate)),
        });

        let mut channel_id: c_ulong = 0;
        let flags = connect_flags;

        unsafe {
            let connect_fn: Symbol<PassThruConnectFn> = library
                .get(b"PassThruConnect\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruConnect - {}", e))?;

            let result = connect_fn(
                device_id,
                protocol_id as c_ulong,
                flags as c_ulong,
                baud_rate as c_ulong,
                &mut channel_id,
            );
            if result != STATUS_NOERROR {
                // Clean up device on failure
                let last_error = get_last_error_message(&library);
                if let Ok(close_fn) = library.get::<PassThruCloseFn>(b"PassThruClose\0") {
                    close_fn(device_id);
                }
                return Err(format!(
                    "ERR_J2534_CONNECT_FAILED: error code {} ({}){}",
                    result,
                    error_code_to_string(result),
                    last_error
                        .map(|s| format!(" - {}", s))
                        .unwrap_or_default()
                ));
            }
        }

        progress_callback(J2534Progress {
            step: "connect_channel".to_string(),
            status: "success".to_string(),
            message: Some(format!("Channel ID: {}", channel_id)),
        });

        // Set up pass-all message filter.
        // CAN protocols need a 4-byte (arb-id) mask/pattern.
        // ISO 9141 / ISO 14230 (K-Line) also need a filter — many J2534 drivers
        // will not deliver any PassThruReadMsgs data until at least one filter is
        // configured.  The can-opener klogger reference uses a 1-byte pass-all.
        let is_can_protocol = protocol_id == PROTOCOL_CAN || protocol_id == PROTOCOL_ISO15765;
        let is_kline_protocol = protocol_id == PROTOCOL_ISO9141 || protocol_id == PROTOCOL_ISO14230;
        let needs_filter = is_can_protocol || is_kline_protocol;

        progress_callback(J2534Progress {
            step: "set_filter".to_string(),
            status: if needs_filter { "in_progress" } else { "success" }.to_string(),
            message: if needs_filter { None } else { Some("Skipped (not required for this protocol)".to_string()) },
        });

        if is_kline_protocol {
            unsafe {
                let filter_fn: Symbol<PassThruStartMsgFilterFn> = library
                    .get(b"PassThruStartMsgFilter\0")
                    .map_err(|e| {
                        format!("ERR_J2534_FUNC_NOT_FOUND: PassThruStartMsgFilter - {}", e)
                    })?;

                // Pass-all filter for K-Line: 1-byte mask=0x00, pattern=0x00
                let mut mask_msg = PassThruMsg::default();
                mask_msg.protocol_id = protocol_id;
                mask_msg.data_size = 1;
                // data[0] already 0 = match everything

                let mut pattern_msg = PassThruMsg::default();
                pattern_msg.protocol_id = protocol_id;
                pattern_msg.data_size = 1;

                let mut filter_id: c_ulong = 0;
                let result = filter_fn(
                    channel_id,
                    PASS_FILTER,
                    &mask_msg,
                    &pattern_msg,
                    std::ptr::null(),
                    &mut filter_id,
                );
                if result != STATUS_NOERROR {
                    eprintln!(
                        "[j2534] Warning: K-Line pass-all filter failed: {} ({}). RX may not work.",
                        result,
                        error_code_to_string(result)
                    );
                }
            }

            progress_callback(J2534Progress {
                step: "set_filter".to_string(),
                status: "success".to_string(),
                message: Some("K-Line pass-all filter".to_string()),
            });
        }

        if is_can_protocol {
            unsafe {
                let filter_fn: Symbol<PassThruStartMsgFilterFn> = library
                    .get(b"PassThruStartMsgFilter\0")
                    .map_err(|e| {
                        format!("ERR_J2534_FUNC_NOT_FOUND: PassThruStartMsgFilter - {}", e)
                    })?;

                // Pass-all filter for 11-bit (standard) CAN IDs
                let mut mask_msg = PassThruMsg::default();
                mask_msg.protocol_id = protocol_id;
                mask_msg.data_size = 4;
                // All zeros = match everything

                let mut pattern_msg = PassThruMsg::default();
                pattern_msg.protocol_id = protocol_id;
                pattern_msg.data_size = 4;
                // All zeros = match everything

                let mut filter_id: c_ulong = 0;
                let result = filter_fn(
                    channel_id,
                    PASS_FILTER,
                    &mask_msg,
                    &pattern_msg,
                    std::ptr::null(),
                    &mut filter_id,
                );
                if result != STATUS_NOERROR {
                    // Clean up on failure
                    if let Ok(disconnect_fn) =
                        library.get::<PassThruDisconnectFn>(b"PassThruDisconnect\0")
                    {
                        disconnect_fn(channel_id);
                    }
                    if let Ok(close_fn) = library.get::<PassThruCloseFn>(b"PassThruClose\0") {
                        close_fn(device_id);
                    }
                    return Err(format!(
                        "ERR_J2534_FILTER_FAILED: error code {} ({})",
                        result,
                        error_code_to_string(result)
                    ));
                }

                // Pass-all filter for 29-bit (extended) CAN IDs
                // CAN_ID_BOTH requires separate filters per ID type
                if (connect_flags & CAN_29BIT_ID) != 0 && connect_flags != CAN_MIXED_CAPTURE_FLAGS {
                    let mut mask_msg_ext = PassThruMsg::default();
                    mask_msg_ext.protocol_id = protocol_id;
                    mask_msg_ext.tx_flags = CAN_29BIT_ID;
                    mask_msg_ext.data_size = 4;

                    let mut pattern_msg_ext = PassThruMsg::default();
                    pattern_msg_ext.protocol_id = protocol_id;
                    pattern_msg_ext.tx_flags = CAN_29BIT_ID;
                    pattern_msg_ext.data_size = 4;

                    let mut filter_id_ext: c_ulong = 0;
                    let result = filter_fn(
                        channel_id,
                        PASS_FILTER,
                        &mask_msg_ext,
                        &pattern_msg_ext,
                        std::ptr::null(),
                        &mut filter_id_ext,
                    );
                    if result != STATUS_NOERROR {
                        eprintln!(
                            "[j2534] Warning: 29-bit pass filter failed: {} ({}). Extended frames may not be received.",
                            result,
                            error_code_to_string(result)
                        );
                    }
                }
            }

            progress_callback(J2534Progress {
                step: "set_filter".to_string(),
                status: "success".to_string(),
                message: None,
            });
        }

        // If CAN_ID_BOTH was requested, try opening a second channel dedicated
        // to 29-bit frames.  Many adapters silently ignore CAN_ID_BOTH in the
        // connect flags, so a dedicated CAN_29BIT_ID channel is the most
        // reliable way to capture extended frames.
        let channel_id_29bit = if connect_flags == CAN_ID_BOTH {
            let mut ch29: c_ulong = 0;
            let connect_fn: Option<Symbol<PassThruConnectFn>> = unsafe {
                library.get(b"PassThruConnect\0").ok()
            };
            let filter_fn_29: Option<Symbol<PassThruStartMsgFilterFn>> = unsafe {
                library.get(b"PassThruStartMsgFilter\0").ok()
            };

            let opened = match (connect_fn, filter_fn_29) {
                (Some(cfn), Some(ffn)) => {
                    let res = unsafe {
                        cfn(
                            device_id,
                            protocol_id as c_ulong,
                            CAN_29BIT_ID as c_ulong,
                            baud_rate as c_ulong,
                            &mut ch29,
                        )
                    };
                    if res == STATUS_NOERROR {
                        // Pass-all filter on the 29-bit channel
                        let mut m = PassThruMsg::default();
                        m.protocol_id = protocol_id;
                        m.tx_flags = CAN_29BIT_ID;
                        m.data_size = 4;
                        let mut p = PassThruMsg::default();
                        p.protocol_id = protocol_id;
                        p.tx_flags = CAN_29BIT_ID;
                        p.data_size = 4;
                        let mut fid: c_ulong = 0;
                        let fres = unsafe {
                            ffn(ch29, PASS_FILTER, &m, &p, std::ptr::null(), &mut fid)
                        };
                        if fres != STATUS_NOERROR {
                            eprintln!(
                                "[j2534] 29-bit channel filter failed: {} ({})",
                                fres,
                                error_code_to_string(fres)
                            );
                        }
                        eprintln!("[j2534] Opened dedicated 29-bit channel (id={})", ch29);
                        Some(ch29)
                    } else {
                        eprintln!(
                            "[j2534] Could not open 29-bit channel: {} ({}) — relying on CAN_ID_BOTH",
                            res,
                            error_code_to_string(res)
                        );
                        None
                    }
                }
                _ => None,
            };
            opened
        } else {
            None
        };

        // Disable loopback by default to prevent TX echo
        let conn = Self {
            library,
            device_id,
            channel_id,
            channel_id_29bit,
            protocol_id,
            sent_messages: Mutex::new(Vec::new()),
        };

        progress_callback(J2534Progress {
            step: "loopback".to_string(),
            status: "in_progress".to_string(),
            message: Some("Disabling loopback (if supported)".to_string()),
        });

        let set_result = conn.set_loopback(false);
        let readback = conn.get_loopback();
        let message = match (set_result, readback) {
            (Ok(()), Ok(state)) => Some(format!(
                "Loopback reported {}",
                if state { "ON" } else { "OFF" }
            )),
            (Ok(()), Err(err)) => Some(format!("Loopback set ok; readback failed: {}", err)),
            (Err(err), Ok(state)) => Some(format!(
                "Loopback set failed: {}; device reports {}",
                err,
                if state { "ON" } else { "OFF" }
            )),
            (Err(err), Err(err2)) => Some(format!(
                "Loopback set failed: {}; readback failed: {}",
                err, err2
            )),
        };

        progress_callback(J2534Progress {
            step: "loopback".to_string(),
            status: "success".to_string(),
            message,
        });

        progress_callback(J2534Progress {
            step: "complete".to_string(),
            status: "success".to_string(),
            message: Some("Connection established".to_string()),
        });

        Ok(conn)
    }

    pub fn send_message(&self, arb_id: u32, data: &[u8], extended: bool) -> Result<(), String> {
        let mut msg = PassThruMsg::default();
        msg.protocol_id = self.protocol_id;
        let is_can = self.protocol_id == PROTOCOL_CAN || self.protocol_id == PROTOCOL_ISO15765;
        let data_len = if is_can {
            msg.tx_flags = if extended { CAN_29BIT_ID } else { 0 };
            msg.data[0] = ((arb_id >> 24) & 0xFF) as u8;
            msg.data[1] = ((arb_id >> 16) & 0xFF) as u8;
            msg.data[2] = ((arb_id >> 8) & 0xFF) as u8;
            msg.data[3] = (arb_id & 0xFF) as u8;

            let data_len = data.len().min(8);
            msg.data[4..4 + data_len].copy_from_slice(&data[..data_len]);
            msg.data_size = (4 + data_len) as u32;
            data_len
        } else {
            msg.tx_flags = 0;
            let data_len = data.len().min(msg.data.len());
            msg.data[..data_len].copy_from_slice(&data[..data_len]);
            msg.data_size = data_len as u32;
            data_len
        };

        let mut num_msgs: c_ulong = 1;

        unsafe {
            let write_fn: Symbol<PassThruWriteMsgsFn> = self
                .library
                .get(b"PassThruWriteMsgs\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruWriteMsgs - {}", e))?;

            let result = write_fn(self.channel_id, &mut msg, &mut num_msgs, 1000);
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_WRITE_FAILED: error code {} [{}]",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        // Track sent message for TX echo filtering (driver workaround)
        // Some drivers don't properly set TX_MSG_TYPE flag on echoed messages
        if let Ok(mut sent) = self.sent_messages.lock() {
            // Clean up old entries (older than 500ms)
            let cutoff = std::time::Instant::now() - std::time::Duration::from_millis(500);
            sent.retain(|m| m.timestamp > cutoff);

            // Add this message
            sent.push(SentMessage {
                arb_id,
                data: data[..data_len].to_vec(),
                timestamp: std::time::Instant::now(),
            });
        }

        Ok(())
    }

    /// Send multiple CAN messages in a single PassThruWriteMsgs call
    /// Returns the number of messages actually sent
    pub fn send_messages_batch(&self, messages: &[(u32, Vec<u8>, bool)]) -> Result<u32, String> {
        if messages.is_empty() {
            return Ok(0);
        }

        // Build array of PassThruMsg
        let mut msg_buffer: Vec<PassThruMsg> = messages
            .iter()
            .map(|(arb_id, data, extended)| {
                let mut msg = PassThruMsg::default();
                msg.protocol_id = self.protocol_id;
                let is_can = self.protocol_id == PROTOCOL_CAN || self.protocol_id == PROTOCOL_ISO15765;
                if is_can {
                    msg.tx_flags = if *extended { CAN_29BIT_ID } else { 0 };
                    msg.data[0] = ((arb_id >> 24) & 0xFF) as u8;
                    msg.data[1] = ((arb_id >> 16) & 0xFF) as u8;
                    msg.data[2] = ((arb_id >> 8) & 0xFF) as u8;
                    msg.data[3] = (arb_id & 0xFF) as u8;

                    let data_len = data.len().min(8);
                    msg.data[4..4 + data_len].copy_from_slice(&data[..data_len]);
                    msg.data_size = (4 + data_len) as u32;
                } else {
                    msg.tx_flags = 0;
                    let data_len = data.len().min(msg.data.len());
                    msg.data[..data_len].copy_from_slice(&data[..data_len]);
                    msg.data_size = data_len as u32;
                }

                msg
            })
            .collect();

        let mut num_msgs: c_ulong = msg_buffer.len() as c_ulong;

        unsafe {
            let write_fn: Symbol<PassThruWriteMsgsFn> = self
                .library
                .get(b"PassThruWriteMsgs\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruWriteMsgs - {}", e))?;

            // Use a longer timeout for batch sends
            let timeout = 5000u32.max(messages.len() as u32 * 10);
            let result = write_fn(
                self.channel_id,
                msg_buffer.as_mut_ptr(),
                &mut num_msgs,
                timeout,
            );
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_WRITE_FAILED: error code {} ({}), sent {}/{} messages",
                    result,
                    error_code_to_string(result),
                    num_msgs,
                    messages.len()
                ));
            }
        }

        // Track sent messages for TX echo filtering (driver workaround)
        if let Ok(mut sent) = self.sent_messages.lock() {
            let cutoff = std::time::Instant::now() - std::time::Duration::from_millis(500);
            sent.retain(|m| m.timestamp > cutoff);

            for (arb_id, data, _) in messages.iter().take(num_msgs as usize) {
                let data_len = if self.protocol_id == PROTOCOL_CAN || self.protocol_id == PROTOCOL_ISO15765 {
                    data.len().min(8)
                } else {
                    data.len()
                };
                sent.push(SentMessage {
                    arb_id: *arb_id,
                    data: data[..data_len].to_vec(),
                    timestamp: std::time::Instant::now(),
                });
            }
        }

        Ok(num_msgs)
    }

    /// Send messages with a custom timeout and return raw J2534 result info
    pub fn write_messages_raw(
        &self,
        messages: &[(u32, Vec<u8>, bool)],
        timeout_ms: u32,
    ) -> Result<RawIoResult, String> {
        if messages.is_empty() {
            return Ok(RawIoResult {
                result: STATUS_NOERROR,
                num_msgs: 0,
            });
        }

        let mut msg_buffer: Vec<PassThruMsg> = messages
            .iter()
            .map(|(arb_id, data, extended)| {
                let mut msg = PassThruMsg::default();
                msg.protocol_id = self.protocol_id;
                let is_can = self.protocol_id == PROTOCOL_CAN || self.protocol_id == PROTOCOL_ISO15765;
                if is_can {
                    msg.tx_flags = if *extended { CAN_29BIT_ID } else { 0 };
                    msg.data[0] = ((arb_id >> 24) & 0xFF) as u8;
                    msg.data[1] = ((arb_id >> 16) & 0xFF) as u8;
                    msg.data[2] = ((arb_id >> 8) & 0xFF) as u8;
                    msg.data[3] = (arb_id & 0xFF) as u8;

                    let data_len = data.len().min(8);
                    msg.data[4..4 + data_len].copy_from_slice(&data[..data_len]);
                    msg.data_size = (4 + data_len) as u32;
                } else {
                    msg.tx_flags = 0;
                    let data_len = data.len().min(msg.data.len());
                    msg.data[..data_len].copy_from_slice(&data[..data_len]);
                    msg.data_size = data_len as u32;
                }

                msg
            })
            .collect();

        let mut num_msgs: c_ulong = msg_buffer.len() as c_ulong;

        let result = unsafe {
            let write_fn: Symbol<PassThruWriteMsgsFn> = self
                .library
                .get(b"PassThruWriteMsgs\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruWriteMsgs - {}", e))?;

            write_fn(
                self.channel_id,
                msg_buffer.as_mut_ptr(),
                &mut num_msgs,
                timeout_ms,
            )
        };

        // Track sent messages for TX echo filtering (driver workaround)
        if let Ok(mut sent) = self.sent_messages.lock() {
            let cutoff = std::time::Instant::now() - std::time::Duration::from_millis(500);
            sent.retain(|m| m.timestamp > cutoff);

            for (arb_id, data, _) in messages.iter().take(num_msgs as usize) {
                let data_len = if self.protocol_id == PROTOCOL_CAN || self.protocol_id == PROTOCOL_ISO15765 {
                    data.len().min(8)
                } else {
                    data.len()
                };
                sent.push(SentMessage {
                    arb_id: *arb_id,
                    data: data[..data_len].to_vec(),
                    timestamp: std::time::Instant::now(),
                });
            }
        }

        Ok(RawIoResult {
            result,
            num_msgs: num_msgs as u32,
        })
    }

    /// Read messages and return raw J2534 result info (no parsing)
    pub fn read_messages_raw(&self, timeout_ms: u32, max_msgs: u32) -> Result<RawIoResult, String> {
        let max_msgs = max_msgs.clamp(1, 64);
        let mut msg_buffer: Vec<PassThruMsg> =
            (0..max_msgs).map(|_| PassThruMsg::default()).collect();
        let mut num_msgs: c_ulong = max_msgs as c_ulong;

        let result = unsafe {
            let read_fn: Symbol<PassThruReadMsgsFn> = self
                .library
                .get(b"PassThruReadMsgs\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruReadMsgs - {}", e))?;

            read_fn(
                self.channel_id,
                msg_buffer.as_mut_ptr(),
                &mut num_msgs,
                timeout_ms,
            )
        };

        Ok(RawIoResult {
            result,
            num_msgs: num_msgs as u32,
        })
    }

    #[allow(dead_code)]
    pub fn read_messages(&self, timeout_ms: u32) -> Result<Vec<CANMessage>, String> {
        self.read_messages_inner(timeout_ms, false)
    }

    pub fn read_messages_with_loopback(&self, timeout_ms: u32) -> Result<Vec<CANMessage>, String> {
        self.read_messages_inner(timeout_ms, true)
    }

    fn read_messages_inner(
        &self,
        timeout_ms: u32,
        include_loopback: bool,
    ) -> Result<Vec<CANMessage>, String> {
        let mut messages = Vec::new();

        // Create message buffer - use Vec since PassThruMsg is large and doesn't implement Copy
        let mut msg_buffer: Vec<PassThruMsg> = (0..16).map(|_| PassThruMsg::default()).collect();
        let mut num_msgs: c_ulong = 16;

        unsafe {
            let read_fn: Symbol<PassThruReadMsgsFn> = self
                .library
                .get(b"PassThruReadMsgs\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruReadMsgs - {}", e))?;

            let result = read_fn(
                self.channel_id,
                msg_buffer.as_mut_ptr(),
                &mut num_msgs,
                timeout_ms,
            );

            // Allow STATUS_NOERROR, ERR_BUFFER_EMPTY, and ERR_TIMEOUT
            // ERR_TIMEOUT can still return messages (e.g., ScanDoc WiFi adapter)
            if result != STATUS_NOERROR && result != ERR_BUFFER_EMPTY && result != ERR_TIMEOUT {
                return Err(format!(
                    "ERR_J2534_READ_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        let is_can = self.protocol_id == PROTOCOL_CAN || self.protocol_id == PROTOCOL_ISO15765;

        for i in 0..num_msgs as usize {
            let msg = &msg_buffer[i];

            // Skip TX echo messages (loopback) - these have TX_MSG_TYPE flag set
            // Unless include_loopback is true (for sanity testing)
            if !include_loopback && (msg.rx_status & TX_MSG_TYPE) != 0 {
                continue;
            }

            if is_can {
                // CAN/ISO15765: first 4 bytes are arb_id, rest is payload
                if msg.data_size < 4 {
                    continue;
                }

                let arb_id = ((msg.data[0] as u32) << 24)
                    | ((msg.data[1] as u32) << 16)
                    | ((msg.data[2] as u32) << 8)
                    | (msg.data[3] as u32);

                let data_len = (msg.data_size - 4) as usize;
                let data = msg.data[4..4 + data_len].to_vec();
                let extended = (msg.rx_status & RX_CAN_29BIT_ID) != 0;

                // Driver workaround: filter out TX echoes by matching against recently sent messages
                let is_tx_echo = if include_loopback {
                    false
                } else if let Ok(mut sent) = self.sent_messages.lock() {
                    let cutoff =
                        std::time::Instant::now() - std::time::Duration::from_millis(500);
                    sent.retain(|m| m.timestamp > cutoff);

                    if let Some(pos) = sent
                        .iter()
                        .position(|m| m.arb_id == arb_id && m.data == data)
                    {
                        sent.remove(pos);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_tx_echo {
                    continue;
                }

                messages.push(CANMessage {
                    timestamp_us: msg.timestamp as u64,
                    arb_id,
                    extended,
                    data,
                    raw_arb_id: arb_id,
                    rx_status: msg.rx_status,
                    data_size: msg.data_size,
                    protocol_id: msg.protocol_id,
                });
            } else {
                // ISO 9141 / ISO 14230 (K-Line): all bytes are raw serial data, no arb_id
                if msg.data_size == 0 {
                    continue;
                }

                let data = msg.data[..msg.data_size as usize].to_vec();

                messages.push(CANMessage {
                    timestamp_us: msg.timestamp as u64,
                    arb_id: 0,
                    extended: false,
                    data,
                    raw_arb_id: 0,
                    rx_status: msg.rx_status,
                    data_size: msg.data_size,
                    protocol_id: msg.protocol_id,
                });
            }
        }

        Ok(messages)
    }

    /// High-throughput drain loop: reads as many messages as possible from the device buffer.
    ///
    /// The first read uses `timeout_ms` to wait for initial data. Subsequent reads use 0ms
    /// timeout to drain any remaining buffered messages without blocking. The loop stops when:
    /// - A read returns fewer messages than `batch_size` (buffer drained)
    /// - `max_drain_reads` iterations have been performed
    /// - A read returns 0 messages
    /// - A non-recoverable error occurs (after the first iteration, partial results are returned)
    ///
    /// TX echoes are filtered using both the rx_status TX_MSG_TYPE flag and the
    /// sent_messages tracking list (driver workaround for adapters that don't set the flag).
    pub fn read_messages_drain(
        &self,
        timeout_ms: u32,
        batch_size: u32,
        max_drain_reads: u32,
    ) -> Result<Vec<CANMessage>, String> {
        let batch_size = batch_size.clamp(1, 256) as usize;
        let max_drain_reads = max_drain_reads.clamp(1, 256) as usize;
        let is_can = self.protocol_id == PROTOCOL_CAN || self.protocol_id == PROTOCOL_ISO15765;
        let mut all_messages = Vec::new();
        let mut msg_buffer: Vec<PassThruMsg> =
            (0..batch_size).map(|_| PassThruMsg::default()).collect();

        let read_fn = unsafe {
            self.library
                .get::<PassThruReadMsgsFn>(b"PassThruReadMsgs\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruReadMsgs - {}", e))?
        };

        for iteration in 0..max_drain_reads {
            let t = if iteration == 0 { timeout_ms } else { 0 };
            let mut num_msgs = batch_size as c_ulong;

            let result = unsafe {
                read_fn(
                    self.channel_id,
                    msg_buffer.as_mut_ptr(),
                    &mut num_msgs,
                    t,
                )
            };

            // Allow STATUS_NOERROR, ERR_BUFFER_EMPTY, and ERR_TIMEOUT
            if result != STATUS_NOERROR && result != ERR_BUFFER_EMPTY && result != ERR_TIMEOUT {
                if iteration == 0 {
                    return Err(format!(
                        "ERR_J2534_READ_FAILED: error code {} ({})",
                        result,
                        error_code_to_string(result)
                    ));
                }
                break; // got some messages already, return what we have
            }

            if num_msgs == 0 {
                break;
            }

            for i in 0..num_msgs as usize {
                let msg = &msg_buffer[i];

                // Skip TX echoes by flag
                if (msg.rx_status & TX_MSG_TYPE) != 0 {
                    continue;
                }

                if is_can {
                    // CAN/ISO15765: first 4 bytes are arb_id, rest is payload
                    if msg.data_size < 4 {
                        continue;
                    }

                    let arb_id = u32::from_be_bytes([
                        msg.data[0],
                        msg.data[1],
                        msg.data[2],
                        msg.data[3],
                    ]);
                    let data_len = (msg.data_size - 4) as usize;
                    let data = msg.data[4..4 + data_len].to_vec();
                    let extended = (msg.rx_status & RX_CAN_29BIT_ID) != 0;

                    // Driver workaround: filter TX echoes by matching sent_messages
                    let is_echo = if let Ok(mut sent) = self.sent_messages.lock() {
                        let cutoff =
                            std::time::Instant::now() - std::time::Duration::from_millis(500);
                        sent.retain(|m| m.timestamp > cutoff);

                        if let Some(pos) =
                            sent.iter().position(|m| m.arb_id == arb_id && m.data == data)
                        {
                            sent.remove(pos);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if is_echo {
                        continue;
                    }

                    all_messages.push(CANMessage {
                        timestamp_us: msg.timestamp as u64,
                        arb_id,
                        extended,
                        data,
                        raw_arb_id: arb_id,
                        rx_status: msg.rx_status,
                        data_size: msg.data_size,
                        protocol_id: msg.protocol_id,
                    });
                } else {
                    // ISO 9141 / ISO 14230 (K-Line): all bytes are raw serial data
                    if msg.data_size == 0 {
                        continue;
                    }

                    let data = msg.data[..msg.data_size as usize].to_vec();

                    all_messages.push(CANMessage {
                        timestamp_us: msg.timestamp as u64,
                        arb_id: 0,
                        extended: false,
                        data,
                        raw_arb_id: 0,
                        rx_status: msg.rx_status,
                        data_size: msg.data_size,
                        protocol_id: msg.protocol_id,
                    });
                }
            }

            // If we got fewer than batch_size, buffer is drained
            if (num_msgs as usize) < batch_size {
                break;
            }
        }

        // Drain the dedicated 29-bit channel (if present)
        if let Some(ch29) = self.channel_id_29bit {
            let mut msg_buffer_29: Vec<PassThruMsg> =
                (0..batch_size).map(|_| PassThruMsg::default()).collect();

            for _ in 0..max_drain_reads {
                let mut num_msgs = batch_size as c_ulong;
                let result = unsafe {
                    read_fn(ch29, msg_buffer_29.as_mut_ptr(), &mut num_msgs, 0)
                };

                if result != STATUS_NOERROR
                    && result != ERR_BUFFER_EMPTY
                    && result != ERR_TIMEOUT
                {
                    break;
                }
                if num_msgs == 0 {
                    break;
                }

                for i in 0..num_msgs as usize {
                    let msg = &msg_buffer_29[i];
                    if (msg.rx_status & TX_MSG_TYPE) != 0 {
                        continue;
                    }
                    if msg.data_size < 4 {
                        continue;
                    }

                    let arb_id = u32::from_be_bytes([
                        msg.data[0],
                        msg.data[1],
                        msg.data[2],
                        msg.data[3],
                    ]);
                    let data_len = (msg.data_size - 4) as usize;
                    let data = msg.data[4..4 + data_len].to_vec();
                    // Always extended on this channel
                    let extended = true;

                    let is_echo = if let Ok(mut sent) = self.sent_messages.lock() {
                        let cutoff =
                            std::time::Instant::now() - std::time::Duration::from_millis(500);
                        sent.retain(|m| m.timestamp > cutoff);
                        if let Some(pos) =
                            sent.iter().position(|m| m.arb_id == arb_id && m.data == data)
                        {
                            sent.remove(pos);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if is_echo {
                        continue;
                    }

                    all_messages.push(CANMessage {
                        timestamp_us: msg.timestamp as u64,
                        arb_id,
                        extended,
                        data,
                        raw_arb_id: arb_id,
                        rx_status: msg.rx_status,
                        data_size: msg.data_size,
                        protocol_id: msg.protocol_id,
                    });
                }

                if (num_msgs as usize) < batch_size {
                    break;
                }
            }
        }

        Ok(all_messages)
    }

    pub fn clear_buffers(&self) -> Result<(), String> {
        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self
                .library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                CLEAR_TX_BUFFER,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_IOCTL_FAILED: CLEAR_TX_BUFFER error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }

            let result = ioctl_fn(
                self.channel_id,
                CLEAR_RX_BUFFER,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_IOCTL_FAILED: CLEAR_RX_BUFFER error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(())
    }

    /// Read version information from the device
    pub fn read_version(&self) -> Result<J2534VersionInfo, String> {
        let mut firmware_version = [0i8; 80];
        let mut dll_version = [0i8; 80];
        let mut api_version = [0i8; 80];

        unsafe {
            let read_version_fn: Symbol<PassThruReadVersionFn> = self
                .library
                .get(b"PassThruReadVersion\0")
                .map_err(|e| {
                    format!("ERR_J2534_FUNC_NOT_FOUND: PassThruReadVersion - {}", e)
                })?;

            let result = read_version_fn(
                self.device_id,
                firmware_version.as_mut_ptr(),
                dll_version.as_mut_ptr(),
                api_version.as_mut_ptr(),
            );

            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_READ_VERSION_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        fn cstr_to_string(arr: &[i8]) -> String {
            let bytes: Vec<u8> = arr
                .iter()
                .take_while(|&&b| b != 0)
                .map(|&b| b as u8)
                .collect();
            String::from_utf8_lossy(&bytes).to_string()
        }

        Ok(J2534VersionInfo {
            firmware_version: cstr_to_string(&firmware_version),
            dll_version: cstr_to_string(&dll_version),
            api_version: cstr_to_string(&api_version),
        })
    }

    /// Get the last error message from the DLL
    pub fn get_last_error(&self) -> Result<String, String> {
        let mut error_msg = [0i8; 80];

        unsafe {
            let get_last_error_fn: Symbol<PassThruGetLastErrorFn> = self
                .library
                .get(b"PassThruGetLastError\0")
                .map_err(|e| {
                    format!("ERR_J2534_FUNC_NOT_FOUND: PassThruGetLastError - {}", e)
                })?;

            get_last_error_fn(error_msg.as_mut_ptr());
        }

        let bytes: Vec<u8> = error_msg
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as u8)
            .collect();
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    /// Read battery voltage (in millivolts, returned as volts)
    pub fn read_battery_voltage(&self) -> Result<f64, String> {
        let mut voltage: u32 = 0;

        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self
                .library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                READ_VBATT,
                std::ptr::null_mut(),
                &mut voltage as *mut u32 as *mut c_void,
            );

            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_READ_VBATT_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(voltage as f64 / 1000.0)
    }

    /// Read programming voltage (in millivolts, returned as volts)
    pub fn read_programming_voltage(&self) -> Result<f64, String> {
        let mut voltage: u32 = 0;

        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self
                .library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                READ_PROG_VOLTAGE,
                std::ptr::null_mut(),
                &mut voltage as *mut u32 as *mut c_void,
            );

            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_READ_PROG_VOLTAGE_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(voltage as f64 / 1000.0)
    }

    /// Start a periodic message
    pub fn start_periodic_message(
        &self,
        arb_id: u32,
        data: &[u8],
        interval_ms: u32,
        extended: bool,
    ) -> Result<u32, String> {
        let mut msg = PassThruMsg::default();
        msg.protocol_id = self.protocol_id;
        msg.tx_flags = if extended { CAN_29BIT_ID } else { 0 };

        // First 4 bytes are the CAN ID
        msg.data[0] = ((arb_id >> 24) & 0xFF) as u8;
        msg.data[1] = ((arb_id >> 16) & 0xFF) as u8;
        msg.data[2] = ((arb_id >> 8) & 0xFF) as u8;
        msg.data[3] = (arb_id & 0xFF) as u8;

        // Copy data bytes (CAN limited to 8 bytes)
        let data_len = data.len().min(8);
        msg.data[4..4 + data_len].copy_from_slice(&data[..data_len]);
        msg.data_size = (4 + data_len) as u32;

        let mut msg_id: c_ulong = 0;

        unsafe {
            let start_periodic_fn: Symbol<PassThruStartPeriodicMsgFn> = self
                .library
                .get(b"PassThruStartPeriodicMsg\0")
                .map_err(|e| {
                    format!(
                        "ERR_J2534_FUNC_NOT_FOUND: PassThruStartPeriodicMsg - {}",
                        e
                    )
                })?;

            let result = start_periodic_fn(self.channel_id, &msg, &mut msg_id, interval_ms);
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_START_PERIODIC_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(msg_id)
    }

    /// Stop a periodic message
    pub fn stop_periodic_message(&self, msg_id: u32) -> Result<(), String> {
        unsafe {
            let stop_periodic_fn: Symbol<PassThruStopPeriodicMsgFn> = self
                .library
                .get(b"PassThruStopPeriodicMsg\0")
                .map_err(|e| {
                    format!(
                        "ERR_J2534_FUNC_NOT_FOUND: PassThruStopPeriodicMsg - {}",
                        e
                    )
                })?;

            let result = stop_periodic_fn(self.channel_id, msg_id);
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_STOP_PERIODIC_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(())
    }

    /// Clear all periodic messages
    pub fn clear_periodic_messages(&self) -> Result<(), String> {
        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self
                .library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                CLEAR_PERIODIC_MSGS,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_CLEAR_PERIODIC_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(())
    }

    /// Add a message filter and return the filter ID
    /// Mask and pattern are padded/truncated to 4 bytes (CAN ID size)
    pub fn add_filter(
        &self,
        filter_type: u32,
        mask: &[u8],
        pattern: &[u8],
        extended: bool,
    ) -> Result<u32, String> {
        let mut mask_msg = PassThruMsg::default();
        mask_msg.protocol_id = self.protocol_id;
        mask_msg.tx_flags = if extended { CAN_29BIT_ID } else { 0 };
        let mask_len = mask.len().min(4);
        mask_msg.data[..mask_len].copy_from_slice(&mask[..mask_len]);
        mask_msg.data_size = 4;

        let mut pattern_msg = PassThruMsg::default();
        pattern_msg.protocol_id = self.protocol_id;
        pattern_msg.tx_flags = if extended { CAN_29BIT_ID } else { 0 };
        let pattern_len = pattern.len().min(4);
        pattern_msg.data[..pattern_len].copy_from_slice(&pattern[..pattern_len]);
        pattern_msg.data_size = 4;

        let mut filter_id: c_ulong = 0;

        unsafe {
            let filter_fn: Symbol<PassThruStartMsgFilterFn> = self
                .library
                .get(b"PassThruStartMsgFilter\0")
                .map_err(|e| {
                    format!("ERR_J2534_FUNC_NOT_FOUND: PassThruStartMsgFilter - {}", e)
                })?;

            let result = filter_fn(
                self.channel_id,
                filter_type,
                &mask_msg,
                &pattern_msg,
                std::ptr::null(),
                &mut filter_id,
            );
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_ADD_FILTER_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(filter_id)
    }

    /// Add a message filter with raw mask/pattern sizes (1-12 bytes)
    pub fn add_filter_raw(
        &self,
        filter_type: u32,
        mask: &[u8],
        pattern: &[u8],
        extended: bool,
    ) -> Result<u32, String> {
        let mask_len = mask.len().min(12);
        let pattern_len = pattern.len().min(12);

        if mask_len == 0 || pattern_len == 0 {
            return Err(
                "ERR_J2534_ADD_FILTER_FAILED: mask/pattern must be 1-12 bytes".to_string(),
            );
        }

        let mut mask_msg = PassThruMsg::default();
        mask_msg.protocol_id = self.protocol_id;
        mask_msg.tx_flags = if extended { CAN_29BIT_ID } else { 0 };
        mask_msg.data[..mask_len].copy_from_slice(&mask[..mask_len]);
        mask_msg.data_size = mask_len as u32;

        let mut pattern_msg = PassThruMsg::default();
        pattern_msg.protocol_id = self.protocol_id;
        pattern_msg.tx_flags = if extended { CAN_29BIT_ID } else { 0 };
        pattern_msg.data[..pattern_len].copy_from_slice(&pattern[..pattern_len]);
        pattern_msg.data_size = pattern_len as u32;

        let mut filter_id: c_ulong = 0;

        unsafe {
            let filter_fn: Symbol<PassThruStartMsgFilterFn> = self
                .library
                .get(b"PassThruStartMsgFilter\0")
                .map_err(|e| {
                    format!("ERR_J2534_FUNC_NOT_FOUND: PassThruStartMsgFilter - {}", e)
                })?;

            let result = filter_fn(
                self.channel_id,
                filter_type,
                &mask_msg,
                &pattern_msg,
                std::ptr::null(),
                &mut filter_id,
            );
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_ADD_FILTER_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(filter_id)
    }

    /// Remove a message filter
    pub fn remove_filter(&self, filter_id: u32) -> Result<(), String> {
        unsafe {
            let stop_filter_fn: Symbol<PassThruStopMsgFilterFn> = self
                .library
                .get(b"PassThruStopMsgFilter\0")
                .map_err(|e| {
                    format!("ERR_J2534_FUNC_NOT_FOUND: PassThruStopMsgFilter - {}", e)
                })?;

            let result = stop_filter_fn(self.channel_id, filter_id);
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_REMOVE_FILTER_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(())
    }

    /// Clear all message filters
    pub fn clear_filters(&self) -> Result<(), String> {
        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self
                .library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                CLEAR_MSG_FILTERS,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_CLEAR_FILTERS_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(())
    }

    /// Get a configuration parameter value
    pub fn get_config(&self, parameter: u32) -> Result<u32, String> {
        let mut config = SConfig {
            parameter,
            value: 0,
        };
        let mut config_list = SConfigList {
            num_of_params: 1,
            config_ptr: &mut config,
        };

        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self
                .library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                GET_CONFIG,
                &mut config_list as *mut SConfigList as *mut c_void,
                std::ptr::null_mut(),
            );

            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_GET_CONFIG_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(config.value)
    }

    /// Set a configuration parameter value
    pub fn set_config(&self, parameter: u32, value: u32) -> Result<(), String> {
        let mut config = SConfig { parameter, value };
        let mut config_list = SConfigList {
            num_of_params: 1,
            config_ptr: &mut config,
        };

        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self
                .library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                SET_CONFIG,
                &mut config_list as *mut SConfigList as *mut c_void,
                std::ptr::null_mut(),
            );

            if result != STATUS_NOERROR {
                return Err(format!(
                    "ERR_J2534_SET_CONFIG_FAILED: error code {} ({})",
                    result,
                    error_code_to_string(result)
                ));
            }
        }

        Ok(())
    }

    /// Get loopback setting
    pub fn get_loopback(&self) -> Result<bool, String> {
        self.get_config(LOOPBACK).map(|v| v != 0)
    }

    /// Set loopback setting
    pub fn set_loopback(&self, enabled: bool) -> Result<(), String> {
        self.set_config(LOOPBACK, if enabled { 1 } else { 0 })
    }

    /// Get the current data rate
    pub fn get_data_rate(&self) -> Result<u32, String> {
        self.get_config(DATA_RATE)
    }

    pub fn fast_init(&self, data: &[u8]) -> Result<CANMessage, String> {
        let mut input = PassThruMsg::default();
        input.protocol_id = self.protocol_id;
        input.data_size = data.len() as u32;
        input.data[..data.len()].copy_from_slice(data);
        let mut output = PassThruMsg::default();

        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self
                .library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                FAST_INIT,
                &mut input as *mut PassThruMsg as *mut c_void,
                &mut output as *mut PassThruMsg as *mut c_void,
            );

            if result != STATUS_NOERROR {
                let last_error = get_last_error_message(&self.library);
                return Err(format!(
                    "ERR_J2534_FAST_INIT_FAILED: error code {} ({}){}",
                    result,
                    error_code_to_string(result),
                    last_error
                        .map(|s| format!(" - {}", s))
                        .unwrap_or_default()
                ));
            }
        }

        Ok(passthru_msg_to_can_message(&output))
    }

    /// Full K-Line init with CC polling — runs entirely in-process.
    pub fn kline_init(
        &self,
        mode: &str,
        fast_init_data: Option<&[u8]>,
        five_baud_address: Option<&[u8]>,
        cc_timeout_ms: u32,
    ) -> Result<KlineInitResult, String> {
        let default_fast: &[u8] = &[0xC1, 0x33, 0xF1, 0x81, 0x66];
        let default_slow: &[u8] = &[0x33];

        match mode {
            "fast" => {
                let data = fast_init_data.unwrap_or(default_fast);
                let resp = self.fast_init(data)?;
                Ok(KlineInitResult {
                    init_method: "fast".into(),
                    detected_protocol: "iso14230-fast".into(),
                    keyword_bytes: Vec::new(),
                    cc_received: false,
                    init_response: vec![resp],
                })
            }
            "slow" => {
                let addr = five_baud_address.unwrap_or(default_slow);
                self.kline_slow_init_with_cc(addr, cc_timeout_ms)
            }
            "auto" => {
                let fast_data = fast_init_data.unwrap_or(default_fast);
                match self.fast_init(fast_data) {
                    Ok(resp) => Ok(KlineInitResult {
                        init_method: "fast".into(),
                        detected_protocol: "iso14230-fast".into(),
                        keyword_bytes: Vec::new(),
                        cc_received: false,
                        init_response: vec![resp],
                    }),
                    Err(fast_err) => {
                        eprintln!("[j2534] kline_init auto: fast init failed: {fast_err}");
                        let addr = five_baud_address.unwrap_or(default_slow);
                        self.kline_slow_init_with_cc(addr, cc_timeout_ms)
                    }
                }
            }
            other => Err(format!("Unknown kline init mode: {other}")),
        }
    }

    /// Five-baud init followed by immediate CC polling (no IPC round-trips).
    fn kline_slow_init_with_cc(
        &self,
        address: &[u8],
        cc_timeout_ms: u32,
    ) -> Result<KlineInitResult, String> {
        let init_resp = self.five_baud_init(address)?;

        // Classify keyword bytes
        let keyword_bytes = init_resp.data.clone();
        let detected_protocol = if keyword_bytes.len() >= 2 {
            let (k1, k2) = (keyword_bytes[0], keyword_bytes[1]);
            if (k1 == 0x08 && k2 == 0x08) || (k1 == 0x94 && k2 == 0x94) {
                "iso9141"
            } else {
                "iso14230-slow"
            }
        } else {
            "unknown"
        };

        // Poll for CC — tight loop, no IPC, directly against the DLL
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(cc_timeout_ms as u64);
        let mut cc_received = false;
        let mut extra_frames: Vec<CANMessage> = Vec::new();

        while std::time::Instant::now() < deadline {
            match self.read_messages_drain(50, 16, 1) {
                Ok(msgs) => {
                    for msg in msgs {
                        if msg.data.contains(&0xCC) {
                            cc_received = true;
                        }
                        extra_frames.push(msg);
                    }
                    if cc_received {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        eprintln!(
            "[j2534] kline_slow_init_with_cc: protocol={} cc={} extra_frames={}",
            detected_protocol, cc_received, extra_frames.len()
        );

        let mut init_response = vec![init_resp];
        init_response.extend(extra_frames);

        Ok(KlineInitResult {
            init_method: "slow".into(),
            detected_protocol: detected_protocol.into(),
            keyword_bytes,
            cc_received,
            init_response,
        })
    }

    pub fn five_baud_init(&self, data: &[u8]) -> Result<CANMessage, String> {
        let mut input_bytes = data.to_vec();
        let mut input = SByteArray {
            num_of_bytes: input_bytes.len() as u32,
            byte_ptr: input_bytes.as_mut_ptr(),
        };
        let mut output_bytes = [0u8; 16];
        let mut output = SByteArray {
            num_of_bytes: output_bytes.len() as u32,
            byte_ptr: output_bytes.as_mut_ptr(),
        };

        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self
                .library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                FIVE_BAUD_INIT,
                &mut input as *mut SByteArray as *mut c_void,
                &mut output as *mut SByteArray as *mut c_void,
            );

            if result != STATUS_NOERROR {
                let last_error = get_last_error_message(&self.library);
                return Err(format!(
                    "ERR_J2534_FIVE_BAUD_INIT_FAILED: error code {} ({}){}",
                    result,
                    error_code_to_string(result),
                    last_error
                        .map(|s| format!(" - {}", s))
                        .unwrap_or_default()
                ));
            }
        }

        let len = (output.num_of_bytes as usize).min(output_bytes.len());
        Ok(CANMessage {
            timestamp_us: 0,
            arb_id: 0,
            extended: false,
            data: output_bytes[..len].to_vec(),
            raw_arb_id: 0,
            rx_status: 0,
            data_size: len as u32,
            protocol_id: self.protocol_id,
        })
    }
}

impl Drop for J2534Connection {
    fn drop(&mut self) {
        unsafe {
            // Disconnect 29-bit channel first (if present)
            if let Some(ch29) = self.channel_id_29bit {
                if let Ok(disconnect_fn) = self
                    .library
                    .get::<PassThruDisconnectFn>(b"PassThruDisconnect\0")
                {
                    disconnect_fn(ch29);
                }
            }
            // Disconnect primary channel
            if let Ok(disconnect_fn) = self
                .library
                .get::<PassThruDisconnectFn>(b"PassThruDisconnect\0")
            {
                disconnect_fn(self.channel_id);
            }
            // Close device
            if let Ok(close_fn) = self.library.get::<PassThruCloseFn>(b"PassThruClose\0") {
                close_fn(self.device_id);
            }
        }
    }
}
