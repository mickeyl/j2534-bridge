use crate::j2534::{self, J2534Connection};
use crate::protocol::{
    CanMessage, KlineInitResult, RawIoResult as ProtocolRawIoResult, Request, Response,
    ResponseData, VersionInfo,
};
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const DEFAULT_RX_CAPACITY: usize = 65_536;
const DEFAULT_POLL_INTERVAL_MS: u64 = 2;
const DEFAULT_PUMP_BATCH_SIZE: u32 = 256;
const DEFAULT_PUMP_MAX_DRAIN_READS: u32 = 16;
const MAX_BATCH_SIZE: usize = 256;
const MAX_DRAIN_READS: usize = 256;

pub struct BridgeWorker {
    tx: Sender<WorkerCommand>,
    join: Option<JoinHandle<()>>,
}

enum WorkerCommand {
    Request {
        request: Request,
        response_tx: SyncSender<Response>,
    },
}

#[derive(Debug, Clone)]
struct WorkerConfig {
    rx_capacity: usize,
    poll_interval: Duration,
    pump_batch_size: u32,
    pump_max_drain_reads: u32,
}

#[derive(Debug, Default)]
struct RxBufferStats {
    dropped_frames: u64,
    high_water_mark: usize,
}

impl WorkerConfig {
    fn from_env() -> Self {
        Self {
            rx_capacity: read_env_usize("J2534_BRIDGE_RX_CAPACITY")
                .unwrap_or(DEFAULT_RX_CAPACITY)
                .max(1),
            poll_interval: Duration::from_millis(
                read_env_u64("J2534_BRIDGE_POLL_INTERVAL_MS")
                    .unwrap_or(DEFAULT_POLL_INTERVAL_MS)
                    .max(1),
            ),
            pump_batch_size: read_env_u32("J2534_BRIDGE_PUMP_BATCH_SIZE")
                .unwrap_or(DEFAULT_PUMP_BATCH_SIZE)
                .clamp(1, MAX_BATCH_SIZE as u32),
            pump_max_drain_reads: read_env_u32("J2534_BRIDGE_PUMP_MAX_DRAIN_READS")
                .unwrap_or(DEFAULT_PUMP_MAX_DRAIN_READS)
                .clamp(1, MAX_DRAIN_READS as u32),
        }
    }
}

impl BridgeWorker {
    pub fn spawn(
        dll_path: String,
        protocol_id: u32,
        baud_rate: u32,
        connect_flags: u32,
    ) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);

        let join = thread::Builder::new()
            .name("j2534-bridge-io".to_string())
            .spawn(move || {
                worker_loop(
                    rx,
                    startup_tx,
                    dll_path,
                    protocol_id,
                    baud_rate,
                    connect_flags,
                );
            })
            .map_err(|e| format!("Failed to start bridge worker: {}", e))?;

        match startup_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                tx,
                join: Some(join),
            }),
            Ok(Err(err)) => {
                let _ = join.join();
                Err(err)
            }
            Err(_) => {
                let _ = join.join();
                Err("Bridge worker terminated before startup completed".to_string())
            }
        }
    }

    pub fn request(&self, request: Request) -> Result<Response, String> {
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        self.tx
            .send(WorkerCommand::Request {
                request,
                response_tx,
            })
            .map_err(|_| "Bridge worker command channel is closed".to_string())?;
        response_rx
            .recv()
            .map_err(|_| "Bridge worker terminated while processing request".to_string())
    }

    pub fn join(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn worker_loop(
    rx: Receiver<WorkerCommand>,
    startup_tx: SyncSender<Result<(), String>>,
    dll_path: String,
    protocol_id: u32,
    baud_rate: u32,
    connect_flags: u32,
) {
    let config = WorkerConfig::from_env();

    let conn = match J2534Connection::open(
        &dll_path,
        protocol_id,
        baud_rate,
        connect_flags,
        |_| {},
    ) {
        Ok(conn) => {
            let _ = startup_tx.send(Ok(()));
            conn
        }
        Err(err) => {
            let _ = startup_tx.send(Err(err));
            return;
        }
    };

    let mut rx_buffer = VecDeque::with_capacity(config.rx_capacity.min(4096));
    let mut stats = RxBufferStats::default();

    loop {
        match rx.recv_timeout(config.poll_interval) {
            Ok(WorkerCommand::Request {
                request,
                response_tx,
            }) => {
                let keep_running = handle_worker_request(
                    &conn,
                    request,
                    &response_tx,
                    &mut rx_buffer,
                    &mut stats,
                    &config,
                );
                if !keep_running {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        pump_frames(&conn, &mut rx_buffer, &mut stats, &config, 0);
    }

    if stats.dropped_frames > 0 {
        eprintln!(
            "[bridge-worker] RX buffer dropped {} frame(s); high-water mark {} / {}",
            stats.dropped_frames, stats.high_water_mark, config.rx_capacity
        );
    }
}

fn handle_worker_request(
    conn: &J2534Connection,
    request: Request,
    response_tx: &SyncSender<Response>,
    rx_buffer: &mut VecDeque<CanMessage>,
    stats: &mut RxBufferStats,
    config: &WorkerConfig,
) -> bool {
    match request {
        Request::EnumerateDevices | Request::Open { .. } => {
            send_response(
                response_tx,
                Response::error(-1, "Request must be handled by the bridge control thread"),
            );
            true
        }

        Request::ReadMessages {
            timeout_ms,
            batch_size,
            max_drain_reads,
        } => {
            let max_messages = compute_read_limit(batch_size, max_drain_reads);
            wait_for_messages(conn, rx_buffer, stats, config, timeout_ms);
            pump_frames(conn, rx_buffer, stats, config, 0);
            let messages = drain_buffer(rx_buffer, max_messages);
            send_response(
                response_tx,
                Response::ok(ResponseData::Messages(messages)),
            );
            true
        }

        Request::ReadMessagesWithLoopback { timeout_ms } => {
            let response = match conn.read_messages_with_loopback(timeout_ms) {
                Ok(messages) => Response::ok(ResponseData::Messages(map_messages(messages))),
                Err(err) => Response::error(-1, err),
            };
            send_response(response_tx, response);
            true
        }

        Request::ReadMessagesRaw {
            timeout_ms,
            max_msgs,
        } => {
            let response = match conn.read_messages_raw(timeout_ms, max_msgs) {
                Ok(raw) => Response::ok(ResponseData::RawIo(map_raw_io(raw))),
                Err(err) => Response::error(-1, err),
            };
            send_response(response_tx, response);
            true
        }

        Request::ClearBuffers => {
            let response = match conn.clear_buffers() {
                Ok(()) => {
                    rx_buffer.clear();
                    Response::ok_none()
                }
                Err(err) => Response::error(-1, err),
            };
            send_response(response_tx, response);
            true
        }

        Request::Close | Request::Shutdown => {
            send_response(response_tx, Response::ok_none());
            false
        }

        other => {
            let response = execute_direct_request(conn, other);
            send_response(response_tx, response);
            true
        }
    }
}

fn execute_direct_request(conn: &J2534Connection, request: Request) -> Response {
    match request {
        Request::SendMessage {
            arb_id,
            data,
            extended,
        } => match conn.send_message(arb_id, &data, extended) {
            Ok(()) => Response::ok_none(),
            Err(err) => Response::error(-1, err),
        },

        Request::SendMessagesBatch { messages } => {
            let msg_tuples: Vec<(u32, Vec<u8>, bool)> = messages
                .into_iter()
                .map(|m| (m.arb_id, m.data, m.extended))
                .collect();
            match conn.send_messages_batch(&msg_tuples) {
                Ok(num_sent) => Response::ok(ResponseData::Number(num_sent)),
                Err(err) => Response::error(-1, err),
            }
        }

        Request::WriteMessagesRaw {
            messages,
            timeout_ms,
        } => {
            let msg_tuples: Vec<(u32, Vec<u8>, bool)> = messages
                .into_iter()
                .map(|m| (m.arb_id, m.data, m.extended))
                .collect();
            match conn.write_messages_raw(&msg_tuples, timeout_ms) {
                Ok(raw) => Response::ok(ResponseData::RawIo(map_raw_io(raw))),
                Err(err) => Response::error(-1, err),
            }
        }

        Request::ReadVersion => match conn.read_version() {
            Ok(version) => Response::ok(ResponseData::Version(map_version_info(version))),
            Err(err) => Response::error(-1, err),
        },

        Request::GetLastError => match conn.get_last_error() {
            Ok(message) => Response::ok(ResponseData::String(message)),
            Err(err) => Response::error(-1, err),
        },

        Request::ReadBatteryVoltage => match conn.read_battery_voltage() {
            Ok(voltage) => Response::ok(ResponseData::Float(voltage)),
            Err(err) => Response::error(-1, err),
        },

        Request::ReadProgrammingVoltage => match conn.read_programming_voltage() {
            Ok(voltage) => Response::ok(ResponseData::Float(voltage)),
            Err(err) => Response::error(-1, err),
        },

        Request::FastInit { data } => match conn.fast_init(&data) {
            Ok(message) => Response::ok(ResponseData::Messages(vec![map_can_message(message)])),
            Err(err) => Response::error(-1, err),
        },

        Request::FiveBaudInit { data } => match conn.five_baud_init(&data) {
            Ok(message) => Response::ok(ResponseData::Messages(vec![map_can_message(message)])),
            Err(err) => Response::error(-1, err),
        },

        Request::KlineInit {
            init_mode,
            fast_init_data,
            five_baud_address,
            cc_timeout_ms,
        } => {
            let mode = match init_mode {
                crate::protocol::KlineInitMode::Fast => "fast",
                crate::protocol::KlineInitMode::Slow => "slow",
                crate::protocol::KlineInitMode::Auto => "auto",
            };
            match conn.kline_init(
                mode,
                fast_init_data.as_deref(),
                five_baud_address.as_deref(),
                cc_timeout_ms.unwrap_or(300),
            ) {
                Ok(result) => Response::ok(ResponseData::KlineInit(map_kline_init_result(result))),
                Err(err) => Response::error(-1, err),
            }
        }

        Request::StartPeriodicMessage {
            arb_id,
            data,
            interval_ms,
            extended,
        } => match conn.start_periodic_message(arb_id, &data, interval_ms, extended) {
            Ok(msg_id) => Response::ok(ResponseData::Number(msg_id)),
            Err(err) => Response::error(-1, err),
        },

        Request::StopPeriodicMessage { msg_id } => match conn.stop_periodic_message(msg_id) {
            Ok(()) => Response::ok_none(),
            Err(err) => Response::error(-1, err),
        },

        Request::ClearPeriodicMessages => match conn.clear_periodic_messages() {
            Ok(()) => Response::ok_none(),
            Err(err) => Response::error(-1, err),
        },

        Request::AddFilter {
            filter_type,
            mask,
            pattern,
            extended,
        } => match parse_filter_type(&filter_type) {
            Ok(filter_type) => match conn.add_filter(filter_type, &mask, &pattern, extended) {
                Ok(filter_id) => Response::ok(ResponseData::Number(filter_id)),
                Err(err) => Response::error(-1, err),
            },
            Err(err) => Response::error(-1, err),
        },

        Request::AddFilterRaw {
            filter_type,
            mask,
            pattern,
            extended,
        } => match parse_filter_type(&filter_type) {
            Ok(filter_type) => match conn.add_filter_raw(filter_type, &mask, &pattern, extended) {
                Ok(filter_id) => Response::ok(ResponseData::Number(filter_id)),
                Err(err) => Response::error(-1, err),
            },
            Err(err) => Response::error(-1, err),
        },

        Request::RemoveFilter { filter_id } => match conn.remove_filter(filter_id) {
            Ok(()) => Response::ok_none(),
            Err(err) => Response::error(-1, err),
        },

        Request::ClearFilters => match conn.clear_filters() {
            Ok(()) => Response::ok_none(),
            Err(err) => Response::error(-1, err),
        },

        Request::GetConfig { parameter } => match conn.get_config(parameter) {
            Ok(value) => Response::ok(ResponseData::Number(value)),
            Err(err) => Response::error(-1, err),
        },

        Request::SetConfig { parameter, value } => match conn.set_config(parameter, value) {
            Ok(()) => Response::ok_none(),
            Err(err) => Response::error(-1, err),
        },

        Request::GetLoopback => match conn.get_loopback() {
            Ok(enabled) => Response::ok(ResponseData::Bool(enabled)),
            Err(err) => Response::error(-1, err),
        },

        Request::SetLoopback { enabled } => match conn.set_loopback(enabled) {
            Ok(()) => Response::ok_none(),
            Err(err) => Response::error(-1, err),
        },

        Request::GetDataRate => match conn.get_data_rate() {
            Ok(data_rate) => Response::ok(ResponseData::Number(data_rate)),
            Err(err) => Response::error(-1, err),
        },

        Request::ReadMessages { .. }
        | Request::ReadMessagesWithLoopback { .. }
        | Request::ReadMessagesRaw { .. }
        | Request::ClearBuffers
        | Request::Close
        | Request::Shutdown
        | Request::EnumerateDevices
        | Request::Open { .. } => {
            Response::error(-1, "Unsupported worker request dispatch")
        }
    }
}

fn wait_for_messages(
    conn: &J2534Connection,
    rx_buffer: &mut VecDeque<CanMessage>,
    stats: &mut RxBufferStats,
    config: &WorkerConfig,
    timeout_ms: u32,
) {
    if !rx_buffer.is_empty() || timeout_ms == 0 {
        return;
    }

    let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
    while rx_buffer.is_empty() {
        let now = Instant::now();
        if now >= deadline {
            break;
        }

        let remaining = deadline.saturating_duration_since(now);
        let wait_slice = remaining.min(config.poll_interval).as_millis().max(1) as u32;
        pump_frames(conn, rx_buffer, stats, config, wait_slice);
    }
}

fn pump_frames(
    conn: &J2534Connection,
    rx_buffer: &mut VecDeque<CanMessage>,
    stats: &mut RxBufferStats,
    config: &WorkerConfig,
    timeout_ms: u32,
) {
    match conn.read_messages_drain(timeout_ms, config.pump_batch_size, config.pump_max_drain_reads)
    {
        Ok(messages) => push_messages(rx_buffer, stats, config.rx_capacity, map_messages(messages)),
        Err(err) => {
            if timeout_ms > 0 {
                eprintln!("[bridge-worker] RX pump error: {}", err);
            }
        }
    }
}

fn push_messages(
    rx_buffer: &mut VecDeque<CanMessage>,
    stats: &mut RxBufferStats,
    capacity: usize,
    messages: Vec<CanMessage>,
) {
    if messages.is_empty() {
        return;
    }

    for message in messages {
        if rx_buffer.len() == capacity {
            rx_buffer.pop_front();
            stats.dropped_frames += 1;
            if stats.dropped_frames == 1 || stats.dropped_frames.is_power_of_two() {
                eprintln!(
                    "[bridge-worker] RX buffer overflow; dropped {} frame(s)",
                    stats.dropped_frames
                );
            }
        }
        rx_buffer.push_back(message);
    }

    stats.high_water_mark = stats.high_water_mark.max(rx_buffer.len());
}

fn drain_buffer(rx_buffer: &mut VecDeque<CanMessage>, max_messages: usize) -> Vec<CanMessage> {
    let count = rx_buffer.len().min(max_messages);
    let mut messages = Vec::with_capacity(count);
    for _ in 0..count {
        if let Some(message) = rx_buffer.pop_front() {
            messages.push(message);
        }
    }
    messages
}

fn compute_read_limit(batch_size: u32, max_drain_reads: u32) -> usize {
    let batch_size = (batch_size as usize).clamp(1, MAX_BATCH_SIZE);
    let max_drain_reads = (max_drain_reads as usize).clamp(1, MAX_DRAIN_READS);
    batch_size.saturating_mul(max_drain_reads)
}

fn parse_filter_type(filter_type: &str) -> Result<u32, String> {
    match filter_type {
        "pass" => Ok(j2534::PASS_FILTER),
        "block" => Ok(j2534::BLOCK_FILTER),
        "flow_control" => Ok(j2534::FLOW_CONTROL_FILTER),
        _ => Err("Invalid filter type".to_string()),
    }
}

fn map_messages(messages: Vec<j2534::CANMessage>) -> Vec<CanMessage> {
    messages.into_iter().map(map_can_message).collect()
}

fn map_can_message(message: j2534::CANMessage) -> CanMessage {
    CanMessage {
        timestamp_us: message.timestamp_us,
        arb_id: message.arb_id,
        extended: message.extended,
        data: message.data,
        raw_arb_id: message.raw_arb_id,
        rx_status: message.rx_status,
        data_size: message.data_size,
        protocol_id: message.protocol_id,
    }
}

fn map_version_info(version: j2534::J2534VersionInfo) -> VersionInfo {
    VersionInfo {
        firmware_version: version.firmware_version,
        dll_version: version.dll_version,
        api_version: version.api_version,
    }
}

fn map_raw_io(raw: j2534::RawIoResult) -> ProtocolRawIoResult {
    ProtocolRawIoResult {
        result: raw.result,
        num_msgs: raw.num_msgs,
    }
}

fn map_kline_init_result(result: j2534::KlineInitResult) -> KlineInitResult {
    KlineInitResult {
        init_method: result.init_method,
        detected_protocol: result.detected_protocol,
        keyword_bytes: result.keyword_bytes,
        cc_received: result.cc_received,
        init_response: map_messages(result.init_response),
    }
}

fn send_response(response_tx: &SyncSender<Response>, response: Response) {
    let _ = response_tx.send(response);
}

fn read_env_u32(name: &str) -> Option<u32> {
    std::env::var(name).ok()?.parse().ok()
}

fn read_env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.parse().ok()
}

fn read_env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok()?.parse().ok()
}
