//! Read-only LAN telemetry JSON-RPC server (Wi‑Fi / LAN add-on API).
//!
//! Binds `0.0.0.0` when enabled. All methods require the local `ipc_token`.
//! Mutating overlay control stays on localhost IPC only.

use crate::ipc::ensure_ipc_token;
use crate::paths;
use crate::telemetry::TelemetryFrame;
use anyhow::Result;
use gridglance_ipc::{
    methods, Request, Response, DEFAULT_LAN_TELEMETRY_PORT, PROTOCOL_VERSION,
};
use serde_json::json;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_CLIENTS: usize = 4;
const MIN_HZ: u32 = 5;
const MAX_HZ: u32 = 30;
const DEFAULT_HZ: u32 = 15;

/// Shared latest frame for LAN clients.
#[derive(Clone, Default)]
pub struct FrameHub {
    inner: Arc<Mutex<Option<TelemetryFrame>>>,
}

impl FrameHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&self, frame: &TelemetryFrame) {
        if let Ok(mut g) = self.inner.lock() {
            *g = Some(frame.clone());
        }
    }

    pub fn snapshot(&self) -> Option<TelemetryFrame> {
        self.inner.lock().ok().and_then(|g| g.clone())
    }
}

/// Controls the LAN telemetry listener lifecycle.
pub struct LanTelemetryServer {
    hub: FrameHub,
    enabled: Arc<AtomicBool>,
    port: Arc<AtomicU16>,
    hz: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    clients: Arc<AtomicUsize>,
    join: Option<JoinHandle<()>>,
    /// Last applied (enabled, port, hz) so sync can no-op.
    applied: Option<(bool, u16, u32)>,
}

impl Default for LanTelemetryServer {
    fn default() -> Self {
        Self {
            hub: FrameHub::new(),
            enabled: Arc::new(AtomicBool::new(false)),
            port: Arc::new(AtomicU16::new(DEFAULT_LAN_TELEMETRY_PORT)),
            hz: Arc::new(AtomicU32::new(DEFAULT_HZ)),
            stop: Arc::new(AtomicBool::new(true)),
            clients: Arc::new(AtomicUsize::new(0)),
            join: None,
            applied: None,
        }
    }
}

impl LanTelemetryServer {
    pub fn publish(&self, frame: &TelemetryFrame) {
        if self.enabled.load(Ordering::Relaxed) {
            self.hub.publish(frame);
        }
    }

    /// Apply settings; restart listener when enable/port/hz change.
    pub fn sync(&mut self, enabled: bool, port: u16, hz: u32) {
        let port = if port == 0 {
            DEFAULT_LAN_TELEMETRY_PORT
        } else {
            port
        };
        let hz = clamp_hz(hz);
        let next = (enabled, port, hz);
        if self.applied == Some(next) {
            return;
        }
        self.stop_listener();
        self.enabled.store(enabled, Ordering::SeqCst);
        self.port.store(port, Ordering::SeqCst);
        self.hz.store(hz, Ordering::SeqCst);
        self.applied = Some(next);
        if enabled {
            if let Err(e) = self.start_listener() {
                eprintln!("[gridglance] LAN telemetry failed to start: {e}");
                self.enabled.store(false, Ordering::SeqCst);
                self.applied = Some((false, port, hz));
            }
        }
    }

    pub fn shutdown(&mut self) {
        self.stop_listener();
        self.enabled.store(false, Ordering::SeqCst);
        self.applied = Some((false, self.port.load(Ordering::Relaxed), self.hz.load(Ordering::Relaxed)));
    }

    fn stop_listener(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Connect to ourselves to unblock accept().
        let port = self.port.load(Ordering::SeqCst);
        if port > 0 {
            let _ = TcpStream::connect(("127.0.0.1", port));
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
        self.clients.store(0, Ordering::SeqCst);
    }

    fn start_listener(&mut self) -> Result<()> {
        let token = ensure_ipc_token()?;
        let port = self.port.load(Ordering::SeqCst);
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(false)?;
        // Short accept timeout so stop flag is noticed without a dummy connect on some OS.
        let _ = listener.set_nonblocking(true);

        self.stop.store(false, Ordering::SeqCst);
        let stop = Arc::clone(&self.stop);
        let hub = self.hub.clone();
        let hz = Arc::clone(&self.hz);
        let clients = Arc::clone(&self.clients);
        let enabled = Arc::clone(&self.enabled);

        self.join = Some(thread::spawn(move || {
            eprintln!(
                "GridGlance LAN telemetry on 0.0.0.0:{port} (token: {})",
                paths::ipc_token_path().display()
            );
            while !stop.load(Ordering::SeqCst) && enabled.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, addr)) => {
                        if stop.load(Ordering::SeqCst) {
                            let _ = stream.shutdown(Shutdown::Both);
                            break;
                        }
                        let n = clients.fetch_add(1, Ordering::SeqCst);
                        if n >= MAX_CLIENTS {
                            clients.fetch_sub(1, Ordering::SeqCst);
                            let _ = stream.shutdown(Shutdown::Both);
                            eprintln!("[gridglance] LAN telemetry: rejected {addr} (max clients)");
                            continue;
                        }
                        let hub_c = hub.clone();
                        let tok = token.clone();
                        let hz_c = Arc::clone(&hz);
                        let clients_c = Arc::clone(&clients);
                        let stop_c = Arc::clone(&stop);
                        thread::spawn(move || {
                            if let Err(e) = handle_client(stream, &tok, hub_c, hz_c, stop_c) {
                                eprintln!("[gridglance] LAN telemetry client error: {e}");
                            }
                            clients_c.fetch_sub(1, Ordering::SeqCst);
                        });
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(40));
                    }
                    Err(e) => {
                        if !stop.load(Ordering::SeqCst) {
                            eprintln!("[gridglance] LAN telemetry accept: {e}");
                            thread::sleep(Duration::from_millis(200));
                        }
                    }
                }
            }
            eprintln!("[gridglance] LAN telemetry stopped");
        }));
        Ok(())
    }
}

pub fn clamp_hz(hz: u32) -> u32 {
    hz.clamp(MIN_HZ, MAX_HZ)
}

fn handle_client(
    stream: TcpStream,
    token: &str,
    hub: FrameHub,
    hz: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_millis(50)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut line = String::new();
    let mut subscribed = false;
    let mut last_push = Instant::now() - Duration::from_secs(1);

    while !stop.load(Ordering::SeqCst) {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let req: Request = match serde_json::from_str(trimmed) {
                    Ok(r) => r,
                    Err(e) => {
                        let resp = Response::err(0, format!("bad request: {e}"));
                        writeln!(writer, "{}", serde_json::to_string(&resp)?)?;
                        continue;
                    }
                };
                let (resp, sub_cmd) = dispatch(&req, token, &hub);
                match sub_cmd {
                    SubCmd::Subscribe => subscribed = true,
                    SubCmd::Unsubscribe => subscribed = false,
                    SubCmd::None => {}
                }
                writeln!(writer, "{}", serde_json::to_string(&resp)?)?;
                writer.flush()?;
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {}
            Err(e) => return Err(e.into()),
        }

        if subscribed {
            let rate = clamp_hz(hz.load(Ordering::Relaxed));
            let interval = Duration::from_secs_f32(1.0 / rate as f32);
            if last_push.elapsed() >= interval {
                if let Some(frame) = hub.snapshot() {
                    let push = json!({
                        "type": "telemetry",
                        "frame": frame,
                    });
                    writeln!(writer, "{}", serde_json::to_string(&push)?)?;
                    writer.flush()?;
                    last_push = Instant::now();
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum SubCmd {
    None,
    Subscribe,
    Unsubscribe,
}

fn dispatch(req: &Request, token: &str, hub: &FrameHub) -> (Response, SubCmd) {
    if !methods::is_lan_allowed(&req.method) {
        return (
            Response::err(
                req.id,
                format!(
                    "method '{}' not allowed on LAN telemetry (read-only)",
                    req.method
                ),
            ),
            SubCmd::None,
        );
    }
    let provided = req.token.as_deref().unwrap_or("");
    if provided != token {
        return (
            Response::err(req.id, "unauthorized: token required"),
            SubCmd::None,
        );
    }

    match req.method.as_str() {
        methods::PING => (
            Response::ok(
                req.id,
                json!({
                    "version": PROTOCOL_VERSION,
                    "backend": "gridglance-overlay",
                    "lan": true,
                    "auth_required": true,
                    "methods": [
                        methods::PING,
                        methods::TELEMETRY_GET,
                        methods::TELEMETRY_SUBSCRIBE,
                        methods::TELEMETRY_UNSUBSCRIBE,
                    ],
                }),
            ),
            SubCmd::None,
        ),
        methods::TELEMETRY_GET => match hub.snapshot() {
            Some(frame) => match serde_json::to_value(&frame) {
                Ok(v) => (Response::ok(req.id, v), SubCmd::None),
                Err(e) => (Response::err(req.id, format!("serialize: {e}")), SubCmd::None),
            },
            None => (
                Response::err(req.id, "no telemetry frame yet"),
                SubCmd::None,
            ),
        },
        methods::TELEMETRY_SUBSCRIBE => (
            Response::ok(
                req.id,
                json!({ "subscribed": true, "type": "telemetry" }),
            ),
            SubCmd::Subscribe,
        ),
        methods::TELEMETRY_UNSUBSCRIBE => (
            Response::ok(req.id, json!({ "subscribed": false })),
            SubCmd::Unsubscribe,
        ),
        other => (
            Response::err(req.id, format!("unknown method '{other}'")),
            SubCmd::None,
        ),
    }
}

/// Test helper: dispatch without a live socket.
#[cfg(test)]
pub fn dispatch_for_test(req: &Request, token: &str, hub: &FrameHub) -> Response {
    dispatch(req, token, hub).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridglance_ipc::methods;

    #[test]
    fn rejects_mutating_methods() {
        let hub = FrameHub::new();
        let req = Request {
            id: 1,
            method: methods::CONFIG_APPLY.into(),
            params: json!({}),
            token: Some("secret".into()),
        };
        let resp = dispatch_for_test(&req, "secret", &hub);
        assert!(!resp.ok);
        assert!(resp.error.as_deref().unwrap_or("").contains("not allowed"));
    }

    #[test]
    fn requires_token() {
        let hub = FrameHub::new();
        let req = Request {
            id: 2,
            method: methods::PING.into(),
            params: json!({}),
            token: None,
        };
        let resp = dispatch_for_test(&req, "secret", &hub);
        assert!(!resp.ok);
    }

    #[test]
    fn ping_ok_with_token() {
        let hub = FrameHub::new();
        let req = Request {
            id: 3,
            method: methods::PING.into(),
            params: json!({}),
            token: Some("secret".into()),
        };
        let resp = dispatch_for_test(&req, "secret", &hub);
        assert!(resp.ok);
        let v = resp.result.unwrap();
        assert_eq!(v["lan"], true);
        assert_eq!(v["auth_required"], true);
    }

    #[test]
    fn hz_clamped() {
        assert_eq!(clamp_hz(1), 5);
        assert_eq!(clamp_hz(15), 15);
        assert_eq!(clamp_hz(100), 30);
    }

    #[test]
    fn smoke_disabled_does_not_bind() {
        let mut srv = LanTelemetryServer::default();
        let probe = TcpListener::bind("127.0.0.1:0").expect("probe bind");
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        srv.sync(false, port, 10);
        assert!(!srv.enabled.load(Ordering::SeqCst));
        assert!(
            TcpStream::connect(("127.0.0.1", port)).is_err(),
            "disabled server must not listen"
        );
    }

    #[test]
    fn smoke_listen_ping() {
        let mut srv = LanTelemetryServer::default();
        // Ephemeral free port to avoid colliding with a running app.
        let probe = TcpListener::bind("127.0.0.1:0").expect("probe bind");
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        srv.sync(true, port, 10);
        assert!(srv.enabled.load(Ordering::SeqCst));

        // Listener starts on a background thread; brief retry for bind race.
        let token = ensure_ipc_token().expect("token");
        let mut stream = None;
        for _ in 0..50 {
            if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
                stream = Some(s);
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let mut stream = stream.expect("connect");
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        let req = Request {
            id: 9,
            method: methods::PING.into(),
            params: json!({}),
            token: Some(token),
        };
        writeln!(stream, "{}", serde_json::to_string(&req).unwrap()).unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).expect("response");
        let resp: Response = serde_json::from_str(line.trim()).expect("json");
        assert!(resp.ok, "{resp:?}");

        srv.shutdown();
        srv.sync(false, port, 10);
        assert!(!srv.enabled.load(Ordering::SeqCst));
    }
}
