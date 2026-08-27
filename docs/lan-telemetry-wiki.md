# LAN telemetry wiki (GridGlance)

How to connect an external app (phone, tablet, second PC) to live iRacing state
from GridGlance over your local Wi‑Fi/LAN.

There is no companion UI yet — clients open a **TCP** socket and speak
**newline-delimited JSON** (NDJSON).

---

## What it is

| Socket | Bind | Purpose |
|--------|------|---------|
| Control IPC | `127.0.0.1:19847` | Localhost only; can mutate config/layout |
| **LAN telemetry** | `0.0.0.0:19848` (default) | Read-only live telemetry for LAN devices |

LAN is **opt-in**, **token-authenticated**, and **read-only**. It does not expose
config, layout, or overlay control. There is no TLS and no internet exposure by
design — use only on trusted local networks.

---

## Enable on the race PC

1. Open **Settings → LAN telemetry**.
2. Turn **Enable LAN telemetry** on.
3. Note **Port** (default `19848`) and **Push rate** (default `15` Hz, range 5–30).
4. Allow **Windows Firewall** if prompted the first time (listens on all interfaces).
5. Under **IPC token**, click **Copy token** (or **Show** to reveal it), then paste
   into the client on your phone/other PC. The same value is also in:

   `%LOCALAPPDATA%\GridGlance\ipc_token`

Toggling enable or changing port restarts the listener without quitting GridGlance.

---

## Find the host from another device

1. Race PC and client must be on the **same Wi‑Fi / LAN**.
2. Use the race PC’s **LAN IPv4** (e.g. `192.168.1.50`), not `127.0.0.1`
   (`127.0.0.1` only works when the client runs on the race PC itself).
3. Connect TCP to `HOST:PORT` (default port `19848`).

On Windows you can find the IP with:

```powershell
ipconfig
```

Look for the active Wi‑Fi / Ethernet adapter’s IPv4 address.

---

## Wire protocol

- Transport: **TCP**
- Framing: **one JSON object per line** (NDJSON), UTF-8, terminated by `\n`
- Each **request** → one **response** line (JSON-RPC style)
- After `telemetry.subscribe`, the server also pushes telemetry lines (no `id`)

### Request

```json
{"id":1,"method":"ping","params":{},"token":"<ipc_token>"}
```

| Field | Required | Notes |
|-------|----------|--------|
| `id` | yes | Echoed in the response; any `u64` |
| `method` | yes | See methods below |
| `params` | no | Object; unused for current LAN methods (`{}` is fine) |
| `token` | **yes on LAN** | Exact contents of `ipc_token` (no extra spaces/newlines) |

### Response

```json
{"id":1,"ok":true,"result":{...}}
```

or

```json
{"id":1,"ok":false,"error":"unauthorized: token required"}
```

Shared types live in `crates/gridglance-ipc`.

---

## Methods

All LAN methods require a valid `token`. Anything else (e.g. `config.apply`) is
rejected as not allowed on the read-only socket.

### `ping`

Health check and capability probe.

```json
{"id":1,"method":"ping","params":{},"token":"..."}
```

Example `result`:

```json
{
  "version": 2,
  "backend": "gridglance-overlay",
  "lan": true,
  "auth_required": true,
  "methods": [
    "ping",
    "telemetry.get",
    "telemetry.subscribe",
    "telemetry.unsubscribe"
  ]
}
```

### `telemetry.get`

Returns the latest `TelemetryFrame` as JSON (snapshot).

```json
{"id":2,"method":"telemetry.get","params":{},"token":"..."}
```

- Success: `result` is the frame object (`connected`, `speed_mps`, `cars`, …).
- Failure: `"no telemetry frame yet"` if GridGlance has not published a frame
  (start the overlay / wait for a session tick).

Field details: see `TelemetryFrame` in
`crates/gridglance-overlay/src/telemetry/mod.rs`.

### `telemetry.subscribe`

Confirms subscription, then the server pushes frames at the configured Hz until
disconnect or unsubscribe.

```json
{"id":3,"method":"telemetry.subscribe","params":{},"token":"..."}
```

Response:

```json
{"id":3,"ok":true,"result":{"subscribed":true,"type":"telemetry"}}
```

Then push lines (no `id`):

```json
{"type":"telemetry","frame":{...}}
```

### `telemetry.unsubscribe`

Stops pushes; connection stays open for further RPC.

```json
{"id":4,"method":"telemetry.unsubscribe","params":{},"token":"..."}
```

---

## Limits

| Limit | Value |
|-------|--------|
| Concurrent clients | 4 (extra connections are closed) |
| Push rate | 5–30 Hz (Settings) |
| Auth | Token required on every method |
| Mutations | Not available on this port |

---

## Examples

Replace `TOKEN`, `HOST`, and `PORT` as needed.

### PowerShell (one-shot ping on the race PC)

```powershell
$token = Get-Content "$env:LOCALAPPDATA\GridGlance\ipc_token" -Raw
$token = $token.Trim()
$hostName = "127.0.0.1"
$port = 19848
$req = (@{ id = 1; method = "ping"; params = @{}; token = $token } | ConvertTo-Json -Compress) + "`n"

$client = New-Object System.Net.Sockets.TcpClient($hostName, $port)
$stream = $client.GetStream()
$writer = New-Object System.IO.StreamWriter($stream)
$reader = New-Object System.IO.StreamReader($stream)
$writer.NewLine = "`n"
$writer.AutoFlush = $true
$writer.Write($req)
$line = $reader.ReadLine()
Write-Output $line
$client.Close()
```

From another device, set `$hostName` to the race PC’s LAN IP.

### Python (subscribe loop)

```python
import json
import os
import socket
from pathlib import Path

HOST = "192.168.1.50"  # race PC LAN IP
PORT = 19848
# On the race PC (Windows): read the file. On other devices: paste the token string.
TOKEN = Path(os.environ["LOCALAPPDATA"], "GridGlance", "ipc_token").read_text().strip()
# TOKEN = "paste-token-here"

def send(sock, method, req_id):
    line = json.dumps({"id": req_id, "method": method, "params": {}, "token": TOKEN}) + "\n"
    sock.sendall(line.encode("utf-8"))

with socket.create_connection((HOST, PORT), timeout=5) as sock:
    sock_file = sock.makefile("rwb", buffering=0)
    send(sock, "telemetry.subscribe", 1)
    print(sock_file.readline().decode().strip())  # RPC ack
    while True:
        line = sock_file.readline()
        if not line:
            break
        msg = json.loads(line)
        if msg.get("type") == "telemetry":
            frame = msg["frame"]
            print(frame.get("lap"), frame.get("speed_mps"))
```

### netcat (if available)

```bash
echo '{"id":1,"method":"ping","params":{},"token":"<ipc_token>"}' | nc 127.0.0.1 19848
echo '{"id":2,"method":"telemetry.get","params":{},"token":"<ipc_token>"}' | nc HOST 19848
```

Keep the connection open after `telemetry.subscribe` to receive push lines.

---

## Troubleshooting

| Symptom | Likely cause |
|---------|----------------|
| Connection refused | LAN telemetry disabled, wrong port, or GridGlance not running |
| Connect works on race PC but not phone | Firewall blocked, wrong LAN IP, or devices on different networks/VLANs |
| `unauthorized: token required` | Missing/wrong `token`, or trailing whitespace in the file/string |
| `method '…' not allowed on LAN telemetry` | Only `ping` / `telemetry.*` are allowed; use localhost `19847` for control |
| `no telemetry frame yet` | Wait for a telemetry tick (overlay running / iRacing or demo producing frames) |
| Connect then immediate close | Client cap (4) reached |
| Subscribe ack then drop / reconnect loop | Older builds closed the socket if the first telemetry push blocked on slow Wi‑Fi. Update GridGlance. |

---

## Out of scope (current)

- WebSockets / HTTP
- TLS or internet exposure
- Remote Settings / layout control over LAN
- Stable “slim” DTO (v1 sends full `TelemetryFrame` JSON)

Update this wiki when the LAN protocol or Settings keys change.
