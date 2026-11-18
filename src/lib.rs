pub mod adapter_inventory;
pub mod can;
pub mod protocol;
pub mod self_test;

#[cfg(windows)]
pub mod client;

#[cfg(windows)]
pub mod c_api;

#[cfg(windows)]
pub mod j2534;

#[cfg(windows)]
pub mod gs_usb;

#[cfg(target_os = "macos")]
pub mod macos;
