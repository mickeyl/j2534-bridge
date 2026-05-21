pub mod adapter_inventory;
pub mod protocol;

#[cfg(windows)]
pub mod client;

#[cfg(windows)]
pub mod j2534;

#[cfg(target_os = "macos")]
pub mod macos;
