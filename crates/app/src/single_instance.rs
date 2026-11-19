//! One app per database. The first instance holds a named mutex and listens
//! on a named pipe; a later launch sends "show" over the pipe and exits.

use std::{
    hash::{Hash, Hasher},
    io::{Read, Write},
    os::windows::io::FromRawHandle,
    path::Path,
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_ALREADY_EXISTS, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE},
    Storage::FileSystem::PIPE_ACCESS_INBOUND,
    System::{
        Pipes::{ConnectNamedPipe, CreateNamedPipeW, PIPE_TYPE_BYTE, PIPE_WAIT},
        Threading::CreateMutexW,
    },
};

pub enum Instance {
    /// This process owns the database. Call [`listen`] once the UI can take messages.
    First { pipe: String },
    /// Another process owns it and was asked to show its window.
    Other,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Names are per database, so the bench (on a copy) runs beside the real app.
pub fn acquire(db: &Path) -> Instance {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    db.to_string_lossy().to_lowercase().hash(&mut hasher);
    let id = format!("Tidemail-{:016x}", hasher.finish());
    let pipe = format!(r"\\.\pipe\{id}");

    // The handle is never closed: the mutex lives as long as the process.
    let mutex =
        unsafe { CreateMutexW(std::ptr::null(), 0, wide(&format!(r"Local\{id}")).as_ptr()) };
    let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if mutex.is_null() || !exists {
        return Instance::First { pipe };
    }
    // The first instance may still be starting; its pipe appears within a moment.
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(&pipe) {
            let _ = f.write_all(b"show");
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Instance::Other
}

/// Call `on_show` (from a background thread) for every later launch.
pub fn listen(pipe: String, on_show: impl Fn() + Send + 'static) {
    std::thread::Builder::new()
        .name("tidemail-instance".into())
        .spawn(move || {
            let name = wide(&pipe);
            loop {
                let handle = unsafe {
                    CreateNamedPipeW(
                        name.as_ptr(),
                        PIPE_ACCESS_INBOUND,
                        PIPE_TYPE_BYTE | PIPE_WAIT,
                        1,
                        0,
                        64,
                        0,
                        std::ptr::null(),
                    )
                };
                if handle == INVALID_HANDLE_VALUE {
                    return;
                }
                let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) } != 0
                    || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                // Owns and closes the handle.
                let mut file = unsafe { std::fs::File::from_raw_handle(handle) };
                let mut buf = [0u8; 16];
                if connected && file.read(&mut buf).is_ok_and(|n| &buf[..n] == b"show") {
                    on_show();
                }
            }
        })
        .expect("spawn instance listener");
}
