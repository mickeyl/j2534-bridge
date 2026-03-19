//! J2534 Bridge Process
//!
//! A standalone process that loads J2534 DLLs and communicates with the main app
//! via named pipes. This allows a 64-bit app to use 32-bit DLLs (via a 32-bit bridge)
//! and vice versa.
//!
//! Usage: j2534-bridge.exe <pipe_name>
//!
//! Environment variables:
//!   J2534_BRIDGE_VERBOSE=1  - Enable verbose logging of all requests

mod j2534;
mod protocol;
mod worker;

use protocol::{DeviceInfo, Message, Request, Response, ResponseData};
use std::io::{BufRead, BufReader, Write};
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(windows)]
use std::os::windows::io::FromRawHandle;

/// Check if verbose logging is enabled via J2534_BRIDGE_VERBOSE env var
fn is_verbose() -> bool {
    static VERBOSE: OnceLock<bool> = OnceLock::new();
    *VERBOSE.get_or_init(|| {
        std::env::var("J2534_BRIDGE_VERBOSE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: j2534-bridge.exe <pipe_name>");
        eprintln!("Example: j2534-bridge.exe \\\\.\\pipe\\cancorder-j2534-1234");
        std::process::exit(1);
    }

    let pipe_name = &args[1];
    eprintln!("[bridge] Starting J2534 bridge on pipe: {}", pipe_name);
    eprintln!(
        "[bridge] Process bitness: {}-bit",
        std::mem::size_of::<usize>() * 8
    );

    if let Err(e) = run_bridge(pipe_name) {
        eprintln!("[bridge] Fatal error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run_bridge(pipe_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    use windows::core::PCSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeA, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
    };

    let pipe_name_cstr = std::ffi::CString::new(pipe_name)?;
    let pipe_handle = unsafe {
        CreateNamedPipeA(
            PCSTR::from_raw(pipe_name_cstr.as_ptr() as *const u8),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            65536,
            65536,
            0,
            None,
        )
    }?;

    eprintln!("[bridge] Waiting for client connection...");

    let connect_result = unsafe { ConnectNamedPipe(pipe_handle, None) };
    if connect_result.is_err() {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(535) {
            let _ = unsafe { CloseHandle(pipe_handle) };
            return Err(format!("Failed to connect pipe: {}", err).into());
        }
    }

    eprintln!("[bridge] Client connected");

    let handle_raw = pipe_handle.0 as *mut std::ffi::c_void;
    let file = unsafe { std::fs::File::from_raw_handle(handle_raw) };

    message_loop(file)
}

#[cfg(not(windows))]
fn run_bridge(_pipe_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    Err("J2534 bridge is only supported on Windows".into())
}

fn message_loop(pipe: std::fs::File) -> Result<(), Box<dyn std::error::Error>> {
    let reader = BufReader::new(pipe.try_clone()?);
    let mut writer = pipe;

    let connection: Arc<Mutex<Option<worker::BridgeWorker>>> = Arc::new(Mutex::new(None));

    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                eprintln!("[bridge] Read error: {}", err);
                break;
            }
        };

        if line.is_empty() {
            continue;
        }

        let msg: Message<Request> = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(err) => {
                eprintln!("[bridge] Parse error: {}", err);
                let response = Message {
                    id: 0,
                    payload: Response::error(-1, format!("Parse error: {}", err)),
                };
                send_response(&mut writer, &response)?;
                continue;
            }
        };

        if is_verbose() {
            eprintln!("[bridge] Request {}: {:?}", msg.id, msg.payload);
        }

        let response_payload = {
            let worker = Arc::clone(&connection);
            let request = msg.payload.clone();
            match panic::catch_unwind(AssertUnwindSafe(|| handle_request(&request, &worker))) {
                Ok(response) => response,
                Err(panic_info) => {
                    let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "Unknown panic".to_string()
                    };
                    eprintln!("[bridge] PANIC while handling request: {}", panic_msg);
                    Response::error(-999, format!("J2534 DLL caused panic: {}", panic_msg))
                }
            }
        };

        let response = Message {
            id: msg.id,
            payload: response_payload,
        };

        send_response(&mut writer, &response)?;

        if matches!(msg.payload, Request::Shutdown) {
            eprintln!("[bridge] Shutdown requested");
            break;
        }
    }

    if let Ok(mut worker_guard) = connection.lock() {
        if let Some(mut worker) = worker_guard.take() {
            worker.join();
        }
    }

    eprintln!("[bridge] Message loop ended");
    Ok(())
}

fn send_response(
    writer: &mut std::fs::File,
    response: &Message<Response>,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(response)?;
    writeln!(writer, "{}", json)?;
    writer.flush()?;
    Ok(())
}

fn handle_request(
    request: &Request,
    connection: &Arc<Mutex<Option<worker::BridgeWorker>>>,
) -> Response {
    match request {
        Request::EnumerateDevices => {
            let devices = j2534::enumerate_devices();
            let device_infos: Vec<DeviceInfo> = devices
                .into_iter()
                .map(|d| DeviceInfo {
                    name: d.name,
                    vendor: d.vendor,
                    dll_path: d.dll_path,
                    can_iso15765: d.can_iso15765,
                    can_iso11898: d.can_iso11898,
                    compatible: d.compatible,
                    bitness: d.bitness,
                    available: d.available,
                    unavailable_reason: d.unavailable_reason,
                    api_version: d.api_version,
                    supported_protocols: d.supported_protocols,
                })
                .collect();
            Response::ok(ResponseData::Devices(device_infos))
        }

        Request::Open {
            dll_path,
            protocol_id,
            baud_rate,
            connect_flags,
        } => {
            let mut worker_guard = connection.lock().unwrap();
            if worker_guard.is_some() {
                return Response::error(-1, "Already connected");
            }

            match worker::BridgeWorker::spawn(
                dll_path.clone(),
                *protocol_id,
                *baud_rate,
                *connect_flags,
            ) {
                Ok(worker) => {
                    *worker_guard = Some(worker);
                    Response::ok(ResponseData::Connected)
                }
                Err(err) => Response::error(-1, err),
            }
        }

        Request::Close => {
            let mut worker_guard = connection.lock().unwrap();
            close_active_worker(&mut worker_guard)
        }

        Request::Shutdown => {
            let mut worker_guard = connection.lock().unwrap();
            shutdown_active_worker(&mut worker_guard)
        }

        _ => {
            let mut worker_guard = connection.lock().unwrap();
            forward_active_worker_request(&mut worker_guard, request.clone())
        }
    }
}

fn close_active_worker(worker: &mut Option<worker::BridgeWorker>) -> Response {
    match worker.take() {
        Some(mut active_worker) => {
            let response = active_worker
                .request(Request::Close)
                .unwrap_or_else(|err| Response::error(-1, err));
            active_worker.join();
            response
        }
        None => Response::ok_none(),
    }
}

fn shutdown_active_worker(worker: &mut Option<worker::BridgeWorker>) -> Response {
    match worker.take() {
        Some(mut active_worker) => {
            let response = active_worker
                .request(Request::Shutdown)
                .unwrap_or_else(|err| Response::error(-1, err));
            active_worker.join();
            response
        }
        None => Response::ok_none(),
    }
}

fn forward_active_worker_request(
    worker: &mut Option<worker::BridgeWorker>,
    request: Request,
) -> Response {
    let Some(active_worker) = worker.as_mut() else {
        return Response::error(-1, "Not connected");
    };

    match active_worker.request(request) {
        Ok(response) => response,
        Err(err) => {
            if let Some(mut dead_worker) = worker.take() {
                dead_worker.join();
            }
            Response::error(-1, err)
        }
    }
}
