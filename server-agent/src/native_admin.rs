//! Tiny loopback HTTP client for the native server's authenticated admin API.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

pub fn request(
    addr: &str,
    token: &str,
    method: &str,
    path: &str,
    body: &str,
) -> Result<serde_json::Value, String> {
    let addr: SocketAddr = addr
        .parse()
        .map_err(|_| "native_admin must be an IP:port".to_string())?;
    if !addr.ip().is_loopback() {
        return Err("native_admin must be loopback".into());
    }
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
        .map_err(|e| format!("native admin is unavailable: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(3))).ok();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(body.as_bytes()))
        .map_err(|e| format!("couldn't write to native admin: {e}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| format!("couldn't read native admin: {e}"))?;
    let text = String::from_utf8(response)
        .map_err(|_| "native admin returned non-UTF8 data".to_string())?;
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or("native admin returned malformed HTTP")?;
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|n| n.parse::<u16>().ok())
        .ok_or("native admin omitted its status")?;
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("native admin returned bad JSON: {e}"))?;
    if !(200..300).contains(&status) {
        return Err(value
            .get("message")
            .or_else(|| value.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("native admin refused the action")
            .to_string());
    }
    Ok(value)
}
