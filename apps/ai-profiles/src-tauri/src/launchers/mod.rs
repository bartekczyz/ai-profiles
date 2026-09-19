pub mod cli;
pub mod gui;
pub mod icons;
pub mod plist;
pub mod script;
pub mod shim;
#[cfg(target_os = "macos")]
mod system_icon;
pub mod wrapper;
