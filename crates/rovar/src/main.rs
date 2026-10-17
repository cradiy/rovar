#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    if std::env::args_os().nth(1).is_some() {
        attach_parent_console();
    }
    rovar::run();
}

#[cfg(windows)]
fn attach_parent_console() {
    use windows_sys::Win32::{
        Storage::FileSystem::{FILE_TYPE_UNKNOWN, GetFileType},
        System::Console::{
            ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
            STD_OUTPUT_HANDLE, SetStdHandle,
        },
    };

    // SAFETY: Called at startup before threads or Rust stdio are initialized.
    // These handles are borrowed from the process table, never owned or closed.
    unsafe {
        let redirected = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].map(|kind| {
            let handle = GetStdHandle(kind);
            (kind, handle, GetFileType(handle) != FILE_TYPE_UNKNOWN)
        });
        // A GUI launch has no console; an agent may only supply redirected pipes.
        // Neither case needs a newly allocated console window.
        if AttachConsole(ATTACH_PARENT_PROCESS) != 0 {
            // Attaching can replace standard handles. Preserve the caller's pipes
            // and files, including independently redirected stdout and stderr.
            for (kind, handle, valid) in redirected {
                if valid {
                    SetStdHandle(kind, handle);
                }
            }
        }
    }
}
