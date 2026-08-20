//! Hidden child-process policy for the one-shot administrator broker.

use std::process::Command;

/// Prevents trusted compatibility tools from creating a visible console while
/// keeping their standard output available to the broker for validation.
#[cfg(windows)]
pub(crate) fn hide_console_window(command: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;

    // CREATE_NO_WINDOW prevents a console-subsystem child from flashing during
    // UAC-approved maintenance or partition verification.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW)
}

/// Keeps broker unit tests portable on non-Windows hosts.
#[cfg(not(windows))]
pub(crate) fn hide_console_window(command: &mut Command) -> &mut Command {
    command
}
