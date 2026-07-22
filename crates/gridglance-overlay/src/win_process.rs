//! Windows helpers so child processes do not flash a console window.

use std::process::Command;

/// CREATE_NO_WINDOW — hide console for nvidia-smi / powershell / etc.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
pub fn no_window(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
pub fn no_window(cmd: &mut Command) -> &mut Command {
    cmd
}
