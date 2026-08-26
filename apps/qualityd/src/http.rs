//! Minimal blocking HTTP/1.1 transport.
//!
//! Studio is a local cockpit: HTML for reviewers and JSON for the API, so this is a
//! bounded `std::net` reader rather than an async stack. It parses a request,
//! hands it to the router, and writes one response. No routing decision and no
//! quality policy lives here.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

/// Largest accepted request line plus headers.
pub const MAX_HEAD_BYTES: usize = 8 * 1024;
/// Largest accepted body.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

/// One parsed request. Query strings are not used by the Studio API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// Upper-case method.
    pub method: String,
    /// Path without query string.
    pub path: String,
    /// UTF-8 body; empty for `GET`.
    pub body: String,
}

/// One response. JSON is the default; the cockpit is HTML/JavaScript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Wire `Content-Type`. API replies stay `application/json`.
    pub content_type: &'static str,
    /// Response body.
    pub body: String,
}

impl HttpResponse {
    /// Response with `status` and a JSON `body`.
    #[must_use]
    pub fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: body.into(),
        }
    }

    /// Exception-first cockpit page.
    #[must_use]
    pub fn html(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: body.into(),
        }
    }

    /// Cockpit script. Policy stays in the JSON API.
    #[must_use]
    pub fn javascript(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            content_type: "text/javascript; charset=utf-8",
            body: body.into(),
        }
    }

    /// `{"error": "..."}` with `status`.
    #[must_use]
    pub fn error(status: u16, message: &str) -> Self {
        let body = serde_json::json!({ "error": message }).to_string();
        Self::new(status, body)
    }

    /// Reason phrase for the status code.
    #[must_use]
    pub fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            201 => "Created",
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            413 => "Payload Too Large",
            422 => "Unprocessable Entity",
            _ => "Internal Server Error",
        }
    }

    /// Serialise to the wire, including headers.
    #[must_use]
    pub fn to_wire(&self) -> String {
        format!(
            "HTTP/1.1 {} {}\r\n\
             Content-Type: {}\r\n\
             Content-Length: {}\r\n\
             Cache-Control: no-store\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            self.status,
            self.reason(),
            self.content_type,
            self.body.len(),
            self.body
        )
    }
}

/// Parse a request line, headers, and body from `reader`.
///
/// Returns `Ok(Err(response))` when the request is malformed or oversized, so the
/// caller can still answer instead of dropping the connection.
///
/// # Errors
///
/// Propagates socket read failures.
pub fn read_request<R: Read>(reader: R) -> io::Result<Result<HttpRequest, HttpResponse>> {
    let mut reader = BufReader::new(reader);
    let mut head = String::new();
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        head.push_str(&line);
        if head.len() > MAX_HEAD_BYTES {
            return Ok(Err(HttpResponse::error(413, "request head too large")));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }
    let mut lines = head.lines();
    let Some(request_line) = lines.next() else {
        return Ok(Err(HttpResponse::error(400, "empty request")));
    };
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(Err(HttpResponse::error(400, "malformed request line")));
    };
    let mut content_length = 0_usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    if content_length > MAX_BODY_BYTES {
        return Ok(Err(HttpResponse::error(413, "request body too large")));
    }
    let mut body = vec![0_u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let Ok(body) = String::from_utf8(body) else {
        return Ok(Err(HttpResponse::error(400, "body must be UTF-8")));
    };
    Ok(Ok(HttpRequest {
        method: method.to_ascii_uppercase(),
        path: target.split('?').next().unwrap_or(target).to_owned(),
        body,
    }))
}

/// Answer one connection with `handler`.
///
/// # Errors
///
/// Propagates socket read/write failures.
pub fn serve_one<H>(stream: &mut TcpStream, handler: H) -> io::Result<()>
where
    H: FnOnce(&HttpRequest) -> HttpResponse,
{
    let peer = stream.try_clone()?;
    let response = match read_request(peer)? {
        Ok(request) => handler(&request),
        Err(response) => response,
    };
    stream.write_all(response.to_wire().as_bytes())?;
    stream.flush()
}

/// Accept connections until `limit` is reached, or forever when `limit` is `None`.
///
/// # Errors
///
/// Propagates listener failures. A single bad connection is skipped, not fatal.
pub fn serve<H>(listener: &TcpListener, limit: Option<usize>, handler: H) -> io::Result<()>
where
    H: Fn(&HttpRequest) -> HttpResponse,
{
    let mut served = 0_usize;
    for stream in listener.incoming() {
        let mut stream = stream?;
        if serve_one(&mut stream, &handler).is_err() {
            continue;
        }
        served += 1;
        if limit.is_some_and(|max| served >= max) {
            break;
        }
    }
    Ok(())
}
