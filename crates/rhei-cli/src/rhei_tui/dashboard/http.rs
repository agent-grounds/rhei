//! The loopback server's wire: reading one request off the socket, writing a
//! reply, and decoding a query string. Every route speaks through these; none
//! of them knows a route. §AR-rhei-viz-flow.7

use std::io::{Read, Write};
use std::net::TcpStream;

pub(super) struct HttpRequest {
    pub(super) path: String,
    pub(super) query: String,
    pub(super) body: Vec<u8>,
}

pub(super) fn read_http_request(stream: &mut TcpStream) -> Option<HttpRequest> {
    const MAX_HEADER_BYTES: usize = 64 * 1024;

    let mut raw = Vec::new();
    let header_end = loop {
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        if raw.len() >= MAX_HEADER_BYTES {
            return None;
        }

        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&buf[..n]);
    };

    let headers = String::from_utf8_lossy(&raw[..header_end]);
    let mut request_line = headers.lines().next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default();
    let target = request_line.next().unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let content_length = if method.eq_ignore_ascii_case("POST") {
        headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0)
    } else {
        0
    };

    // POST bodies may arrive after the first read or exceed the 8 KiB request
    // buffer; read the declared body before dispatching mutation routes. §AR-rhei-viz-flow.7.
    let body_start = header_end + 4;
    let mut body = raw.get(body_start..).unwrap_or_default().to_vec();
    if body.len() < content_length {
        let already = body.len();
        body.resize(content_length, 0);
        stream.read_exact(&mut body[already..]).ok()?;
    } else {
        body.truncate(content_length);
    }

    Some(HttpRequest { path: path.to_string(), query: query.to_string(), body })
}

pub(super) fn write_response(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

pub(super) fn write_not_found(stream: &mut TcpStream) {
    write_status(stream, "404 Not Found", b"not found");
}

pub(super) fn write_status(stream: &mut TcpStream, status: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

/// Find the first `key=value` pair in a query string and percent-decode its value.
pub(super) fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (k == key).then(|| percent_decode(v))
    })
}

pub(super) fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 3;
                }
                _ => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(super) fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(super) fn escape_json_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
