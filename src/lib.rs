pub mod adapter_inventory;
pub mod protocol;
pub mod self_test;

#[cfg(windows)]
pub mod client;

#[cfg(windows)]
pub mod j2534;

#[cfg(target_os = "macos")]
pub mod macos;
