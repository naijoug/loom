//! Isolated Chromium for one website research run.
//!
//! The profile, process group and loopback debugging port belong to this run.
//! An already running user browser is never attached to or signalled.
use crate::tool_process::{self, Detached, DetachedSpec, ProcessOwner};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use std::{
    net::IpAddr,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    pub url: String,
    pub origin: String,
}

pub fn normalize_website_url(input: &str) -> Result<Site, String> {
    let input = input.trim();
    if input.is_empty()
        || input.len() > 2048
        || input
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return Err("invalid_website_url".into());
    }
    let lower = input.to_ascii_lowercase();
    if lower.starts_with("javascript:")
        || lower.starts_with("file:")
        || lower.starts_with("data:")
        || lower.starts_with("chrome:")
        || lower.starts_with("about:")
    {
        return Err("unsupported_website_scheme".into());
    }
    let (scheme, rest) = if let Some(rest) = input.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = input.strip_prefix("http://") {
        ("http", rest)
    } else {
        return Err("unsupported_website_scheme".into());
    };
    if rest.contains('@') {
        return Err("website_credentials_rejected".into());
    }
    let (authority, raw_path) = rest
        .split_once('/')
        .map(|(host, path)| (host, format!("/{path}")))
        .unwrap_or((rest, "/".into()));
    if authority.is_empty() || authority.starts_with('[') {
        return Err("invalid_website_url".into());
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => (
            host,
            Some(port.parse::<u16>().map_err(|_| "invalid_website_url")?),
        ),
        _ => (authority, None),
    };
    if host.is_empty() || host.contains(':') {
        return Err("invalid_website_url".into());
    }
    let port = port.unwrap_or(if scheme == "https" { 443 } else { 80 });
    if port == 0 {
        return Err("invalid_website_url".into());
    }
    if !host_allowed(host) {
        return Err("website_private_target_rejected".into());
    }
    let path = raw_path.split('#').next().unwrap_or("/");
    let (path, query) = path
        .split_once('?')
        .map(|(path, query)| (path, Some(query)))
        .unwrap_or((path, None));
    if path.is_empty() {
        return Err("invalid_website_url".into());
    }
    let origin = display_origin(scheme, host, port);
    let mut url = format!("{origin}{path}");
    if let Some(query) = query {
        url.push('?');
        url.push_str(query);
    }
    Ok(Site { url, origin })
}

fn host_allowed(host: &str) -> bool {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return !ip_is_private(ip) || loopback_tests_allowed(ip);
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return cfg!(test);
    }
    !host.is_empty()
}

fn loopback_tests_allowed(ip: IpAddr) -> bool {
    cfg!(test) && ip.is_loopback()
}

fn ip_is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_unspecified()
                || (a == 100 && (64..=127).contains(&b))
                || a == 0
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

fn display_origin(scheme: &str, host: &str, port: u16) -> String {
    let default = if scheme == "https" { 443 } else { 80 };
    if port == default {
        format!("{scheme}://{host}")
    } else {
        format!("{scheme}://{host}:{port}")
    }
}

pub fn origin_of(url: &str) -> Result<String, String> {
    Ok(normalize_website_url(url)?.origin)
}

pub fn resolve_browser(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return accept_browser(path);
    }
    if let Ok(path) = std::env::var("LOOM_CHROME") {
        if !path.is_empty() {
            return accept_browser(Path::new(&path));
        }
    }
    let candidates = [
        "/usr/local/bin/google-chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];
    for candidate in candidates {
        let path = Path::new(candidate);
        if path.is_file() {
            return accept_browser(path);
        }
    }
    Err("browser_unavailable".into())
}

fn accept_browser(path: &Path) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(path).map_err(|_| "browser_unavailable")?;
    let meta = std::fs::symlink_metadata(&canonical).map_err(|_| "browser_unavailable")?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("browser_unavailable".into());
    }
    Ok(canonical)
}

pub struct Session {
    process: Detached,
    endpoint: String,
    page_id: String,
    page_url: String,
    page_socket: String,
}

impl Session {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn page_id(&self) -> &str {
        &self.page_id
    }
    pub fn page_url(&self) -> &str {
        &self.page_url
    }
    #[cfg(test)]
    pub fn pid(&self) -> u32 {
        self.process.pid()
    }

    pub async fn screenshot(
        &self,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<Vec<u8>, String> {
        if cancelled() {
            return Err("cancelled".into());
        }
        let port = endpoint_port(&self.endpoint)?;
        let shot = cdp_sequence(
            &self.page_socket,
            port,
            &[
                (1, "Page.enable", "{}"),
                (2, "Page.captureScreenshot", r#"{"format":"png"}"#),
            ],
            cancelled,
        )
        .await?;
        let data = shot
            .pointer("/result/data")
            .and_then(|value| value.as_str())
            .ok_or("browser_screenshot_failed")?;
        let bytes = STANDARD
            .decode(data)
            .map_err(|_| "browser_screenshot_failed")?;
        if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || bytes.len() > 10 * 1024 * 1024
        {
            return Err("browser_screenshot_rejected".into());
        }
        Ok(bytes)
    }

    pub async fn close(self) -> Result<(), String> {
        self.process.close().await
    }
}

pub async fn launch(
    executable: &Path,
    profile: &Path,
    site: &Site,
    owner: ProcessOwner,
    operation_id: &str,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Session, String> {
    let executable = accept_browser(executable)?;
    if !profile.is_absolute() {
        return Err("invalid_browser_profile".into());
    }
    std::fs::create_dir_all(profile).map_err(|error| error.to_string())?;
    let cwd = profile.parent().ok_or("invalid_browser_profile")?;
    let mut process = Detached::spawn(DetachedSpec {
        operation_id: operation_id.into(),
        owner,
        program: executable,
        args: vec![
            "--headless=new".into(),
            "--disable-gpu".into(),
            "--disable-dev-shm-usage".into(),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
            "--disable-extensions".into(),
            "--disable-sync".into(),
            "--remote-debugging-address=127.0.0.1".into(),
            "--remote-debugging-port=0".into(),
            "--remote-allow-origins=*".into(),
            format!("--user-data-dir={}", profile.display()),
            site.url.clone(),
        ],
        cwd: cwd.to_path_buf(),
    })?;
    match wait_until_ready(&mut process, profile, site, cancelled).await {
        Ok(page) => Ok(Session {
            process,
            endpoint: page.endpoint,
            page_id: page.page_id,
            page_url: page.page_url,
            page_socket: page.page_socket,
        }),
        Err(error) => {
            let closed = process.close().await;
            Err(match closed {
                Ok(()) => error,
                Err(closed) => format!("{error}; {closed}"),
            })
        }
    }
}

struct ReadyPage {
    endpoint: String,
    page_id: String,
    page_url: String,
    page_socket: String,
}

async fn wait_until_ready(
    process: &mut Detached,
    profile: &Path,
    site: &Site,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<ReadyPage, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let port_file = profile.join("DevToolsActivePort");
    let port = loop {
        if cancelled() {
            return Err("cancelled".into());
        }
        if let Some(code) = process.exited()? {
            return Err(format!("browser_exited_before_ready:{code}"));
        }
        if let Ok(text) = std::fs::read_to_string(&port_file) {
            if let Some(port) = text
                .lines()
                .next()
                .and_then(|line| line.parse::<u16>().ok())
            {
                if port != 0 {
                    break port;
                }
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err("browser_cdp_timeout".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    tool_process::require_owned_loopback_port(process.pid(), port)?;
    let endpoint = format!("http://127.0.0.1:{port}");
    let mut version = Err("browser_http_failed".into());
    for _ in 0..20 {
        if cancelled() {
            return Err("cancelled".into());
        }
        version = http_json(&endpoint, "/json/version", cancelled).await;
        if version.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let version = version?;
    let debugger = version["webSocketDebuggerUrl"].as_str().unwrap_or("");
    if !debugger.starts_with(&format!("ws://127.0.0.1:{port}/")) {
        return Err("cdp_endpoint_rejected".into());
    }
    let list = http_json(&endpoint, "/json/list", cancelled).await?;
    let pages = list.as_array().ok_or("browser_target_missing")?;
    let matched = pages.iter().find(|page| {
        page["type"] == "page"
            && page["url"]
                .as_str()
                .and_then(|url| origin_of(url).ok())
                .as_deref()
                == Some(site.origin.as_str())
    });
    let Some(page) = matched else {
        let observed = pages.iter().find_map(|page| {
            if page["type"] == "page" {
                page["url"].as_str().and_then(|url| origin_of(url).ok())
            } else {
                None
            }
        });
        return Err(format!(
            "cross_origin_redirect:{}",
            observed.unwrap_or_else(|| "none".into())
        ));
    };
    let page_id = page["id"]
        .as_str()
        .ok_or("browser_target_missing")?
        .to_string();
    let page_url = page["url"]
        .as_str()
        .ok_or("browser_target_missing")?
        .to_string();
    let socket = page["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("cdp_endpoint_rejected")?;
    Ok(ReadyPage {
        endpoint,
        page_id,
        page_url,
        page_socket: accept_page_socket(socket, port)?,
    })
}

fn endpoint_port(endpoint: &str) -> Result<u16, String> {
    endpoint
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse().ok())
        .ok_or_else(|| "cdp_endpoint_rejected".into())
}

fn accept_page_socket(url: &str, port: u16) -> Result<String, String> {
    let prefix = format!("ws://127.0.0.1:{port}");
    let Some(path) = url.strip_prefix(&prefix) else {
        return Err("cdp_endpoint_rejected".into());
    };
    if path.contains('@') || !path.starts_with("/devtools/page/") {
        return Err("cdp_endpoint_rejected".into());
    }
    Ok(url.into())
}

async fn http_json(
    endpoint: &str,
    path: &str,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Value, String> {
    let port = endpoint_port(endpoint)?;
    let text = loopback_http(
        port,
        &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"),
        cancelled,
    )
    .await?;
    let body = text
        .split("\r\n\r\n")
        .nth(1)
        .or_else(|| text.split("\n\n").nth(1))
        .ok_or_else(|| {
            format!(
                "browser_http_failed:{}",
                text.chars().take(120).collect::<String>()
            )
        })?;
    serde_json::from_str(body.trim()).map_err(|_| {
        format!(
            "browser_http_failed:{}",
            body.chars().take(120).collect::<String>()
        )
    })
}

async fn cdp_sequence(
    socket: &str,
    port: u16,
    calls: &[(u64, &str, &str)],
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Value, String> {
    let path = socket
        .strip_prefix(&format!("ws://127.0.0.1:{port}"))
        .ok_or("cdp_endpoint_rejected")?;
    let payloads: Vec<String> = calls
        .iter()
        .map(|(id, method, params)| {
            format!(r#"{{"id":{id},"method":"{method}","params":{params}}}"#)
        })
        .collect();
    let reply = websocket_text(port, path, &payloads, cancelled).await?;
    serde_json::from_str(&reply).map_err(|_| "browser_cdp_failed".into())
}

async fn loopback_http(
    port: u16,
    request: &str,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<String, String> {
    if cancelled() {
        return Err("cancelled".into());
    }
    let request = request.to_string();
    tokio::task::spawn_blocking(move || read_http(port, &request))
        .await
        .map_err(|_| "browser_http_failed:join".to_string())?
}

fn read_http(port: u16, request: &str) -> Result<String, String> {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_secs(2),
    )
    .map_err(|error| format!("browser_http_failed:connect:{error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(request.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|error| format!("browser_http_failed:write:{error}"))?;
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        if bytes.len() >= 1024 * 1024 {
            break;
        }
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(error) => return Err(format!("browser_http_failed:read:{error}")),
        }
    }
    if bytes.is_empty() {
        return Err(format!("browser_http_failed:empty:{port}"));
    }
    String::from_utf8(bytes).map_err(|_| "browser_http_failed:utf8".into())
}

async fn websocket_text(
    port: u16,
    path: &str,
    payloads: &[String],
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    if cancelled() {
        return Err("cancelled".into());
    }
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(|_| "browser_cdp_failed")?;
    let key = STANDARD.encode(crate::models::now_ms().to_le_bytes());
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\nOrigin: http://127.0.0.1:{port}\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|_| "browser_cdp_failed")?;
    stream.flush().await.map_err(|_| "browser_cdp_failed")?;
    let mut header = Vec::new();
    let mut byte = [0; 1];
    while !header.windows(4).any(|window| window == b"\r\n\r\n") && header.len() < 8192 {
        let n = stream
            .read(&mut byte)
            .await
            .map_err(|_| "browser_cdp_failed")?;
        if n == 0 {
            return Err("browser_cdp_failed".into());
        }
        header.push(byte[0]);
    }
    let header = String::from_utf8_lossy(&header);
    if !header.starts_with("HTTP/1.1 101") {
        return Err("browser_cdp_failed".into());
    }
    let mut final_reply = None;
    for payload in payloads {
        write_client_frame(&mut stream, payload.as_bytes()).await?;
        let reply = read_server_text(&mut stream).await?;
        final_reply = Some(reply);
    }
    final_reply.ok_or_else(|| "browser_cdp_failed".into())
}

async fn write_client_frame(
    stream: &mut tokio::net::TcpStream,
    payload: &[u8],
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    if payload.len() > 1024 * 1024 {
        return Err("browser_cdp_failed".into());
    }
    let mask = crate::models::now_ms().to_le_bytes();
    let mut frame = vec![0x81];
    if payload.len() < 126 {
        frame.push(0x80 | payload.len() as u8);
    } else {
        frame.push(0x80 | 126);
        frame.extend((payload.len() as u16).to_be_bytes());
    }
    frame.extend(&mask[..4]);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    stream
        .write_all(&frame)
        .await
        .map_err(|_| "browser_cdp_failed")?;
    stream
        .flush()
        .await
        .map_err(|_| "browser_cdp_failed".into())
}

async fn read_server_text(stream: &mut tokio::net::TcpStream) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let mut header = [0; 2];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|_| "browser_cdp_failed")?;
    if header[0] & 0x0f != 1 || header[1] & 0x80 != 0 {
        return Err("browser_cdp_failed".into());
    }
    let length = match header[1] & 0x7f {
        127 => return Err("browser_cdp_failed".into()),
        126 => {
            let mut extended = [0; 2];
            stream
                .read_exact(&mut extended)
                .await
                .map_err(|_| "browser_cdp_failed")?;
            u16::from_be_bytes(extended) as usize
        }
        short => short as usize,
    };
    if length > 12 * 1024 * 1024 {
        return Err("browser_screenshot_rejected".into());
    }
    let mut payload = vec![0; length];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|_| "browser_cdp_failed")?;
    String::from_utf8(payload).map_err(|_| "browser_cdp_failed".into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "loom-browser-{}-{}-{}",
            std::process::id(),
            crate::models::now_ms(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_fake(path: &Path) {
        std::fs::write(
            path,
            r#"#!/usr/bin/env python3
import base64, hashlib, json, os, socket, time
from pathlib import Path
args = sys_argv = __import__("sys").argv[1:]
profile = next(arg.split("=", 1)[1] for arg in args if arg.startswith("--user-data-dir="))
url = args[-1]
mode = Path(profile).parent.joinpath("mode").read_text().strip() if Path(profile).parent.joinpath("mode").exists() else "serve"
Path(profile).mkdir(parents=True, exist_ok=True)
if mode == "exit":
    raise SystemExit(3)
if mode == "slow":
    time.sleep(30)
    raise SystemExit(0)
if mode == "spoof":
    Path(profile, "DevToolsActivePort").write_text(Path(profile).parent.joinpath("spoof-port").read_text().strip() + "\n/devtools/browser/spoof\n")
    time.sleep(30)
    raise SystemExit(0)
observed = url
if mode == "redirect":
    observed = Path(profile).parent.joinpath("redirect-url").read_text().strip()
sock = socket.socket()
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.bind(("127.0.0.1", 0))
port = sock.getsockname()[1]
Path(profile, "DevToolsActivePort").write_text(f"{port}\n/devtools/browser/fake\n")
sock.listen(32)
page_id = "page-owned"
png = base64.b64encode(bytes.fromhex("89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000a49444154789c63000100000500010d0a2db40000000049454e44ae426082")).decode()
def answer(conn, data):
    text = data.decode("latin1")
    if "Upgrade: websocket" in text:
        key = next(line.split(":",1)[1].strip() for line in text.split("\r\n") if line.lower().startswith("sec-websocket-key:"))
        accept = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()).decode()
        conn.send(f"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n".encode())
        while True:
            header = conn.recv(2)
            if len(header) < 2: return
            length = header[1] & 0x7f
            if length == 126:
                length = int.from_bytes(conn.recv(2), "big")
            mask = conn.recv(4)
            payload = bytes(byte ^ mask[i % 4] for i, byte in enumerate(conn.recv(length)))
            message = json.loads(payload.decode())
            if message.get("method") == "Page.captureScreenshot":
                body = json.dumps({"id": message["id"], "result": {"data": png}}).encode()
            else:
                body = json.dumps({"id": message["id"], "result": {}}).encode()
            frame = bytes([0x81, len(body)]) + body
            conn.send(frame)
        return
    path = text.split(" ")[1]
    if path.startswith("/json/version"):
        body = {"Browser": "FakeChrome/1", "webSocketDebuggerUrl": f"ws://127.0.0.1:{port}/devtools/browser/fake"}
    elif path.startswith("/json/list"):
        body = [{"id": page_id, "type": "page", "url": observed, "webSocketDebuggerUrl": f"ws://127.0.0.1:{port}/devtools/page/{page_id}"}]
    else:
        body = {}
    raw = json.dumps(body).encode()
    conn.send(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: %d\r\nConnection: close\r\n\r\n" % len(raw) + raw)
while True:
    conn, _ = sock.accept()
    data = b""
    conn.settimeout(5)
    try:
        while b"\r\n\r\n" not in data and len(data) < 65536:
            chunk = conn.recv(4096)
            if not chunk: break
            data += chunk
        if data:
            answer(conn, data)
    except Exception:
        pass
    conn.close()
"#,
        )
        .unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn public_urls_are_normalized_and_private_or_active_urls_are_rejected() {
        let site = normalize_website_url("https://example.com/docs?token=secret#part").unwrap();
        assert_eq!(site.origin, "https://example.com");
        assert_eq!(site.url, "https://example.com/docs?token=secret");
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://user:pw@example.com/a",
            "http://10.1.2.3/",
            "http://192.168.1.9/",
            "http://169.254.1.1/",
        ] {
            assert!(normalize_website_url(bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn fake_browser_binds_its_own_port_and_closing_it_spares_another_process() {
        let root = temp();
        let browser = root.join("fake-chrome");
        write_fake(&browser);
        let mut sibling = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let site = normalize_website_url("http://127.0.0.1:9/item").unwrap();
        let session = launch(
            &browser,
            &root.join("browser-profile"),
            &site,
            ProcessOwner::Feature {
                feature: "research".into(),
                scope_id: "research-b".into(),
                run_id: "run-b".into(),
            },
            "browser-serve",
            &|| false,
        )
        .await
        .unwrap();
        assert_eq!(session.page_id(), "page-owned");
        let png = session.screenshot(&|| false).await.unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        let pid = session.pid();
        session.close().await.unwrap();
        assert!(std::process::Command::new("kill")
            .args(["-0", &sibling.id().to_string()])
            .status()
            .unwrap()
            .success());
        let _ = sibling.kill();
        let _ = sibling.wait();
        assert!(std::fs::read_to_string(format!("/proc/{pid}/stat")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn exit_spoof_redirect_and_cancel_are_distinct() {
        let root = temp();
        let browser = root.join("fake-chrome");
        write_fake(&browser);
        let owner = ProcessOwner::Feature {
            feature: "research".into(),
            scope_id: "research-b".into(),
            run_id: "run-b".into(),
        };
        let site = normalize_website_url("http://127.0.0.1:9/item").unwrap();
        std::fs::write(root.join("mode"), "exit").unwrap();
        let exited = match launch(
            &browser,
            &root.join("profile-exit"),
            &site,
            owner.clone(),
            "browser-exit",
            &|| false,
        )
        .await
        {
            Ok(_) => panic!("exit fixture launched"),
            Err(error) => error,
        };
        assert!(
            exited.starts_with("browser_exited_before_ready:3"),
            "{exited}"
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        std::fs::write(
            root.join("spoof-port"),
            listener.local_addr().unwrap().port().to_string(),
        )
        .unwrap();
        std::fs::write(root.join("mode"), "spoof").unwrap();
        let spoofed = match launch(
            &browser,
            &root.join("profile-spoof"),
            &site,
            owner.clone(),
            "browser-spoof",
            &|| false,
        )
        .await
        {
            Ok(_) => panic!("spoof fixture launched"),
            Err(error) => error,
        };
        assert!(spoofed.contains("browser_port_not_owned"), "{spoofed}");
        drop(listener);
        std::fs::write(root.join("redirect-url"), "http://127.0.0.1:99/away").unwrap();
        std::fs::write(root.join("mode"), "redirect").unwrap();
        let redirected = match launch(
            &browser,
            &root.join("profile-redirect"),
            &site,
            owner.clone(),
            "browser-redirect",
            &|| false,
        )
        .await
        {
            Ok(_) => panic!("redirect fixture launched"),
            Err(error) => error,
        };
        assert!(
            redirected.starts_with("cross_origin_redirect:http://127.0.0.1:99"),
            "{redirected}"
        );
        std::fs::write(root.join("mode"), "slow").unwrap();
        let cancelled = match launch(
            &browser,
            &root.join("profile-slow"),
            &site,
            owner,
            "browser-cancel",
            &|| true,
        )
        .await
        {
            Ok(_) => panic!("cancel fixture launched"),
            Err(error) => error,
        };
        assert!(cancelled.starts_with("cancelled"), "{cancelled}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    #[ignore = "real Chrome controlled-site launch; not part of the default suite"]
    async fn real_chrome_observes_a_local_page_and_explains_a_redirect() {
        let browser = resolve_browser(None).expect("Chrome is installed for this ignored check");
        let root = temp();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(8) {
                let mut stream = stream.unwrap();
                use std::io::{Read, Write};
                let mut buffer = [0; 1024];
                let _ = stream.read(&mut buffer);
                let body = "ok";
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{body}"
                );
            }
        });
        let mut sibling = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let site = normalize_website_url(&format!("http://127.0.0.1:{port}/")).unwrap();
        let session = launch(
            &browser,
            &root.join("profile"),
            &site,
            ProcessOwner::Feature {
                feature: "research".into(),
                scope_id: "r".into(),
                run_id: "run".into(),
            },
            "chrome-real",
            &|| false,
        )
        .await
        .unwrap();
        assert!(session.page_url().starts_with(&site.origin));
        assert!(session
            .screenshot(&|| false)
            .await
            .unwrap()
            .starts_with(b"\x89PNG"));
        session.close().await.unwrap();
        assert!(std::process::Command::new("kill")
            .args(["-0", &sibling.id().to_string()])
            .status()
            .unwrap()
            .success());
        let _ = sibling.kill();
        let _ = sibling.wait();
        let other = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let other_port = other.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in other.incoming().take(4) {
                let mut stream = stream.unwrap();
                use std::io::{Read, Write};
                let mut buffer = [0; 512];
                let _ = stream.read(&mut buffer);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nno"
                );
            }
        });
        let redirector = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let from = redirector.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in redirector.incoming().take(4) {
                let mut stream = stream.unwrap();
                use std::io::{Read, Write};
                let mut buffer = [0; 512];
                let _ = stream.read(&mut buffer);
                let _ = write!(stream, "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{other_port}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
        });
        let start = normalize_website_url(&format!("http://127.0.0.1:{from}/")).unwrap();
        let error = match launch(
            &browser,
            &root.join("profile-redirect"),
            &start,
            ProcessOwner::Feature {
                feature: "research".into(),
                scope_id: "r".into(),
                run_id: "run-2".into(),
            },
            "chrome-redirect",
            &|| false,
        )
        .await
        {
            Ok(_) => panic!("redirect stayed on the original origin"),
            Err(error) => error,
        };
        assert!(
            error.starts_with(&format!(
                "cross_origin_redirect:http://127.0.0.1:{other_port}"
            )),
            "{error}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
