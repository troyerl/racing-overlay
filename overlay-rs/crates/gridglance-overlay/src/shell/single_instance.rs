//! Localhost TCP single-instance guard (Python `single_instance` parity).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Control port for activate-existing (IPC is 19847).
pub const CONTROL_PORT: u16 = 19848;
const ACTIVATE: &[u8] = b"activate\n";

/// Holds the listen socket thread; drop does not stop the process.
pub struct SingleInstanceGuard {
    _alive: Arc<AtomicBool>,
}

/// If another instance is running, send activate and return `None`.
/// Otherwise become primary and invoke `on_activate` when peers connect.
pub fn acquire(on_activate: impl Fn() + Send + Sync + 'static) -> Option<SingleInstanceGuard> {
    if try_activate_existing() {
        return None;
    }
    let listener = match TcpListener::bind(("127.0.0.1", CONTROL_PORT)) {
        Ok(l) => l,
        Err(_) => {
            // Stale bind race — try activate once more, else proceed unlocked.
            if try_activate_existing() {
                return None;
            }
            return Some(SingleInstanceGuard {
                _alive: Arc::new(AtomicBool::new(true)),
            });
        }
    };
    let _ = listener.set_nonblocking(false);
    let alive = Arc::new(AtomicBool::new(true));
    let alive_bg = alive.clone();
    let on_activate = Arc::new(on_activate);
    thread::spawn(move || {
        while alive_bg.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0u8; 64];
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
                    if let Ok(n) = stream.read(&mut buf) {
                        let msg = &buf[..n];
                        if msg.windows(ACTIVATE.len()).any(|w| w == ACTIVATE)
                            || msg.windows(8).any(|w| w == b"activate")
                        {
                            on_activate();
                        }
                    }
                }
                Err(_) => thread::sleep(Duration::from_millis(50)),
            }
        }
    });
    Some(SingleInstanceGuard { _alive: alive })
}

fn try_activate_existing() -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], CONTROL_PORT)),
        Duration::from_millis(350),
    ) else {
        return false;
    };
    let _ = stream.set_write_timeout(Some(Duration::from_millis(350)));
    let _ = stream.write_all(ACTIVATE);
    let _ = stream.flush();
    true
}
