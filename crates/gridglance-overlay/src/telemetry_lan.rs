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
use serde::Serialize;
use serde_json::json;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_CLIENTS: usize = 4;
const MIN_HZ: u32 = 5;
const MAX_HZ: u32 = 30;
const DEFAULT_HZ: u32 = 15;
const WRITE_DEADLINE: Duration = Duration::from_secs(20);
const IDLE_SLEEP: Duration = Duration::from_millis(5);

/// Decrements the live-client counter even if the handler panics.
struct ClientSlot(Arc<AtomicUsize>);

impl Drop for ClientSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Serialize)]
struct TelemetryPush<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    frame: &'a TelemetryFrame,
}

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
                            let _slot = ClientSlot(clients_c);
                            if let Err(e) = handle_client(stream, addr, &tok, hub_c, hz_c, stop_c) {
                                eprintln!("[gridglance] LAN telemetry {addr}: {e}");
                            }
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

/// Best-effort LAN IPv4 for display (interface used for outbound traffic).
/// Does not send packets; connect selects a route so `local_addr` is the LAN IP.
pub fn preferred_lan_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // Any routable address works; no datagrams are sent.
    socket.connect("8.8.8.8:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// `host:port` string for LAN clients (falls back to a placeholder host).
pub fn lan_connect_endpoint(port: u16) -> String {
    match preferred_lan_ipv4() {
        Some(ip) => format!("{ip}:{port}"),
        None => format!("<lan-ip>:{port}"),
    }
}

fn io_would_block(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
    )
}

fn io_gone(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        ErrorKind::BrokenPipe
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected
            | ErrorKind::UnexpectedEof
    )
}

fn flush_deadline(stream: &mut TcpStream, deadline: Instant) -> Result<()> {
    loop {
        match stream.flush() {
            Ok(()) => return Ok(()),
            Err(e) if io_would_block(&e) => {
                if Instant::now() >= deadline {
                    anyhow::bail!("write timeout");
                }
                thread::sleep(IDLE_SLEEP);
            }
            Err(e) if io_gone(&e) => return Err(e.into()),
            Err(e) => return Err(e.into()),
        }
    }
}

fn write_all_deadline(stream: &mut TcpStream, mut buf: &[u8], deadline: Instant) -> Result<()> {
    while !buf.is_empty() {
        match stream.write(buf) {
            Ok(0) => anyhow::bail!("write zero"),
            Ok(n) => buf = &buf[n..],
            Err(e) if io_would_block(&e) => {
                if Instant::now() >= deadline {
                    anyhow::bail!("write timeout");
                }
                thread::sleep(IDLE_SLEEP);
            }
            Err(e) if io_gone(&e) => return Err(e.into()),
            Err(e) => return Err(e.into()),
        }
    }
    flush_deadline(stream, deadline)
}

fn write_line(stream: &mut TcpStream, json: &str) -> Result<()> {
    let mut line = String::with_capacity(json.len() + 1);
    line.push_str(json);
    line.push('\n');
    write_all_deadline(stream, line.as_bytes(), Instant::now() + WRITE_DEADLINE)
}

/// Write a telemetry line. `Ok(false)` means the socket wasn't writable — skip
/// this frame and keep the connection (don't start a partial NDJSON line).
fn try_push_line(stream: &mut TcpStream, json: &str) -> Result<bool> {
    let mut line = String::with_capacity(json.len() + 1);
    line.push_str(json);
    line.push('\n');
    let bytes = line.as_bytes();
    let n = match stream.write(bytes) {
        Ok(n) => n,
        Err(e) if io_would_block(&e) => return Ok(false),
        Err(e) if io_gone(&e) => return Err(e.into()),
        Err(e) => return Err(e.into()),
    };
    if n == 0 {
        return Ok(false);
    }
    write_all_deadline(stream, &bytes[n..], Instant::now() + WRITE_DEADLINE)?;
    Ok(true)
}

fn handle_client(
    stream: TcpStream,
    addr: SocketAddr,
    token: &str,
    hub: FrameHub,
    hz: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    stream.set_nodelay(true)?;
    stream.set_nonblocking(true)?;

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut line = String::new();
    let mut subscribed = false;
    // Don't dump a (often huge) frame in the same tick as the subscribe ack —
    // the client is still reading that RPC line. Immediate blocking writes on
    // a slow Wi‑Fi path were closing the socket right after the ack.
    let mut last_push = Instant::now();
    let mut logged_ser = false;

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
                        write_line(&mut writer, &serde_json::to_string(&resp)?)?;
                        continue;
                    }
                };
                let (resp, sub_cmd) = dispatch(&req, token, &hub);
                match sub_cmd {
                    SubCmd::Subscribe => {
                        subscribed = true;
                        last_push = Instant::now();
                    }
                    SubCmd::Unsubscribe => subscribed = false,
                    SubCmd::None => {}
                }
                write_line(&mut writer, &serde_json::to_string(&resp)?)?;
            }
            Err(e) if io_would_block(&e) => thread::sleep(IDLE_SLEEP),
            Err(e) if io_gone(&e) => break,
            Err(e) => return Err(e.into()),
        }

        if subscribed {
            let rate = clamp_hz(hz.load(Ordering::Relaxed));
            let interval = Duration::from_secs_f32(1.0 / rate as f32);
            if last_push.elapsed() >= interval {
                if let Some(frame) = hub.snapshot() {
                    let push = TelemetryPush {
                        kind: "telemetry",
                        frame: &frame,
                    };
                    match serde_json::to_string(&push) {
                        Ok(s) => match try_push_line(&mut writer, &s) {
                            Ok(true) => last_push = Instant::now(),
                            Ok(false) => {
                                // Client hasn't drained yet; skip this tick.
                            }
                            Err(e) => return Err(e),
                        },
                        Err(e) => {
                            if !logged_ser {
                                eprintln!(
                                    "[gridglance] LAN telemetry {addr}: skip frame serialize ({e})"
                                );
                                logged_ser = true;
                            }
                            last_push = Instant::now();
                        }
                    }
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
    fn telemetry_get_includes_tire_sets() {
        let hub = FrameHub::new();
        hub.publish(&TelemetryFrame {
            connected: true,
            tire_sets: crate::telemetry::TireSets {
                available: Some(2),
                used: Some(1),
                dry_limit: Some(3),
                left_available: Some(2),
                right_available: Some(1),
                ..Default::default()
            },
            ..Default::default()
        });
        let req = Request {
            id: 4,
            method: methods::TELEMETRY_GET.into(),
            params: json!({}),
            token: Some("secret".into()),
        };
        let resp = dispatch_for_test(&req, "secret", &hub);
        assert!(resp.ok);
        let v = resp.result.unwrap();
        assert_eq!(v["tire_sets"]["available"], 2);
        assert_eq!(v["tire_sets"]["used"], 1);
        assert_eq!(v["tire_sets"]["dry_limit"], 3);
        assert_eq!(v["tire_sets"]["left_available"], 2);
        assert_eq!(v["tire_sets"]["right_available"], 1);
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

    fn connect_retry(port: u16) -> TcpStream {
        let mut stream = None;
        for _ in 0..50 {
            if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
                stream = Some(s);
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        stream.expect("connect")
    }

    #[test]
    fn smoke_subscribe_stays_open() {
        let mut srv = LanTelemetryServer::default();
        let probe = TcpListener::bind("127.0.0.1:0").expect("probe bind");
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        srv.sync(true, port, 10);
        srv.publish(&TelemetryFrame {
            connected: true,
            speed_mps: 21.0,
            ..Default::default()
        });

        let token = ensure_ipc_token().expect("token");
        let mut stream = connect_retry(port);
        stream.set_nodelay(true).ok();
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();

        let sub = Request {
            id: 1,
            method: methods::TELEMETRY_SUBSCRIBE.into(),
            params: json!({}),
            token: Some(token.clone()),
        };
        writeln!(stream, "{}", serde_json::to_string(&sub).unwrap()).unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).expect("subscribe ack");
        let ack: Response = serde_json::from_str(line.trim()).expect("ack json");
        assert!(ack.ok, "{ack:?}");
        assert_eq!(ack.result.as_ref().unwrap()["subscribed"], true);

        line.clear();
        reader.read_line(&mut line).expect("telemetry push");
        let push: serde_json::Value = serde_json::from_str(line.trim()).expect("push json");
        assert_eq!(push["type"], "telemetry");
        assert_eq!(push["frame"]["speed_mps"], 21.0);

        // Connection must still accept RPC after the first push (the old bug
        // closed the socket immediately after the subscribe ack).
        let ping = Request {
            id: 2,
            method: methods::PING.into(),
            params: json!({}),
            token: Some(token),
        };
        writeln!(reader.get_mut(), "{}", serde_json::to_string(&ping).unwrap()).unwrap();
        let mut got_ping = false;
        for _ in 0..30 {
            line.clear();
            reader.read_line(&mut line).expect("line after subscribe");
            if let Ok(resp) = serde_json::from_str::<Response>(line.trim()) {
                if resp.id == 2 {
                    assert!(resp.ok, "{resp:?}");
                    got_ping = true;
                    break;
                }
            }
        }
        assert!(got_ping, "socket closed or no ping reply after subscribe");

        srv.shutdown();
    }

    #[test]
    fn smoke_subscribe_nan_frame_keeps_socket() {
        let mut srv = LanTelemetryServer::default();
        let probe = TcpListener::bind("127.0.0.1:0").expect("probe bind");
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        srv.sync(true, port, 10);
        srv.publish(&TelemetryFrame {
            speed_mps: f32::NAN,
            ..Default::default()
        });

        let token = ensure_ipc_token().expect("token");
        let mut stream = connect_retry(port);
        stream.set_nodelay(true).ok();
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();

        let sub = Request {
            id: 1,
            method: methods::TELEMETRY_SUBSCRIBE.into(),
            params: json!({}),
            token: Some(token.clone()),
        };
        writeln!(stream, "{}", serde_json::to_string(&sub).unwrap()).unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).expect("subscribe ack");
        let ack: Response = serde_json::from_str(line.trim()).expect("ack json");
        assert!(ack.ok, "{ack:?}");

        let ping = Request {
            id: 2,
            method: methods::PING.into(),
            params: json!({}),
            token: Some(token),
        };
        writeln!(reader.get_mut(), "{}", serde_json::to_string(&ping).unwrap()).unwrap();
        let mut got_ping = false;
        for _ in 0..30 {
            line.clear();
            reader.read_line(&mut line).expect("line after subscribe");
            if let Ok(resp) = serde_json::from_str::<Response>(line.trim()) {
                if resp.id == 2 {
                    assert!(resp.ok, "{resp:?}");
                    got_ping = true;
                    break;
                }
            }
        }
        assert!(got_ping, "socket closed or no ping reply after NaN frame");

        srv.shutdown();
    }
}
