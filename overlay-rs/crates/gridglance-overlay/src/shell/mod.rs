//! Process shell: single-instance guard + system tray.

mod single_instance;
mod tray;

pub use single_instance::acquire as acquire_instance;
pub use tray::{poll_events, spawn as spawn_tray, TrayCommand};
