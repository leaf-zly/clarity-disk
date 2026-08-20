//! Windows process-launch policy for platform adapters.
//!
//! Read-only discovery still has a few compatibility providers implemented as
//! PowerShell scripts. They must run as invisible child processes so a normal
//! desktop interaction never flashes a console window.

use std::process::Command;

/// Configures a trusted Windows child process to run without creating a
/// console window.
///
/// The caller remains responsible for selecting a trusted executable and for
/// redirecting standard handles. This function only changes the Windows
/// creation flags and has no effect on non-Windows builds.
#[cfg(windows)]
pub(crate) fn hide_console_window(command: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;

    // CREATE_NO_WINDOW is intentionally explicit instead of relying on the
    // parent subsystem; Tauri can be launched from a terminal during testing.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW)
}

/// Leaves process configuration unchanged on non-Windows targets so shared
/// adapter code remains testable without platform-specific branching.
#[cfg(not(windows))]
pub(crate) fn hide_console_window(command: &mut Command) -> &mut Command {
    command
}
