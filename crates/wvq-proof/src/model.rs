//! Opt-in loopback model adapter for explorer/triage escape packets.
//!
//! Normal verification never calls this module. A caller must explicitly ask
//! for an AI decision, configure a loopback OpenAI-compatible endpoint, and
//! pass through [`AiCostFirewall`] before any bytes leave the process.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::{AiCall, AiCallKind, AiCostFirewall, BudgetExhausted};

const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

/// Fixed local model endpoint and cost metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalModelConfig {
    /// OpenAI-compatible `http://localhost/.../chat/completions` endpoint.
    pub endpoint: String,
    /// Model identity sent to the local server.
    pub model: String,
    /// Maximum tokens the server may generate.
    pub max_output_tokens: u64,
    /// Input price, in micros per one million tokens.
    pub input_micros_per_million: u64,
    /// Output price, in micros per one million tokens.
    pub output_micros_per_million: u64,
}

/// One explicit model escape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalModelRequest {
    /// Budget axis to charge.
    pub kind: AiCallKind,
    /// Bounded text packet. The adapter does not read repository files.
    pub prompt: String,
}

/// Measured model reply and usage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalModelReply {
    /// Model identity returned by the endpoint, or the configured identity.
    pub model: String,
    /// Assistant text.
    pub text: String,
    /// Server-reported input tokens.
    pub input_tokens: u64,
    /// Server-reported output tokens.
    pub output_tokens: u64,
    /// Calculated money usage in micros.
    pub cost_micros: u64,
}

/// Local endpoint, protocol, usage, or budget failure.
#[derive(Debug, Error)]
pub enum ModelError {
    /// Only loopback HTTP endpoints are accepted.
    #[error("invalid local model endpoint: {0}")]
    Endpoint(String),
    /// TCP/HTTP I/O failed.
    #[error("local model transport: {0}")]
    Io(#[from] std::io::Error),
    /// Endpoint returned malformed or incomplete evidence.
    #[error("local model response: {0}")]
    Response(String),
    /// AI Cost Firewall refused the call before model execution.
    #[error(transparent)]
    Budget(#[from] BudgetExhausted),
}

/// Call a loopback OpenAI-compatible chat completion endpoint.
///
/// The worst-case prompt+output reservation is checked on a cloned firewall
/// before the TCP connection is opened. Only measured server usage is charged
/// to the real firewall after a successful response.
///
/// # Errors
///
/// Fails before network I/O when the endpoint is not loopback or the budget
/// cannot cover the configured worst case. Missing usage fields fail closed.
pub fn call_local_model(
    config: &LocalModelConfig,
    request: &LocalModelRequest,
    firewall: &mut AiCostFirewall,
) -> Result<LocalModelReply, ModelError> {
    let endpoint = ParsedEndpoint::parse(&config.endpoint)?;
    if request.prompt.is_empty() || request.prompt.len() > 256 * 1024 {
        return Err(ModelError::Response(
            "prompt must contain 1..=262144 UTF-8 bytes".into(),
        ));
    }
    if config.model.trim().is_empty() || config.max_output_tokens == 0 {
        return Err(ModelError::Endpoint(
            "model and max_output_tokens are required".into(),
        ));
    }
    // UTF-8 bytes are a conservative tokenizer-independent upper bound for
    // ordinary text token counts; the preflight may over-reserve, never under.
    let estimated_input = u64::try_from(request.prompt.len()).unwrap_or(u64::MAX);
    let reserved_tokens = estimated_input.saturating_add(config.max_output_tokens);
    let reserved_cost = token_cost(
        estimated_input,
        config.max_output_tokens,
        config.input_micros_per_million,
        config.output_micros_per_million,
    );
    let mut reservation = firewall.clone();
    reservation.charge(&AiCall {
        kind: request.kind,
        tokens: reserved_tokens,
        cost_micros: reserved_cost,
    })?;

    let body = serde_json::to_vec(&json!({
        "model": config.model,
        "messages": [{"role": "user", "content": request.prompt}],
        "max_tokens": config.max_output_tokens,
        "stream": false
    }))
    .map_err(|err| ModelError::Response(err.to_string()))?;
    let response = endpoint.post(&body)?;
    let parsed: ChatResponse = serde_json::from_slice(&response)
        .map_err(|err| ModelError::Response(format!("invalid JSON: {err}")))?;
    let usage = parsed
        .usage
        .ok_or_else(|| ModelError::Response("usage evidence is missing".into()))?;
    let text = parsed
        .choices
        .first()
        .map(|choice| choice.message.content.clone())
        .filter(|text| !text.is_empty())
        .ok_or_else(|| ModelError::Response("assistant content is missing".into()))?;
    let tokens = usage.prompt_tokens.saturating_add(usage.completion_tokens);
    if tokens > reserved_tokens || usage.completion_tokens > config.max_output_tokens {
        return Err(ModelError::Response(
            "reported usage exceeds the preflight reservation".into(),
        ));
    }
    let cost_micros = token_cost(
        usage.prompt_tokens,
        usage.completion_tokens,
        config.input_micros_per_million,
        config.output_micros_per_million,
    );
    firewall.charge(&AiCall {
        kind: request.kind,
        tokens,
        cost_micros,
    })?;
    Ok(LocalModelReply {
        model: parsed.model.unwrap_or_else(|| config.model.clone()),
        text,
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        cost_micros,
    })
}

fn token_cost(input: u64, output: u64, input_rate: u64, output_rate: u64) -> u64 {
    input
        .saturating_mul(input_rate)
        .saturating_add(output.saturating_mul(output_rate))
        .div_ceil(1_000_000)
}

struct ParsedEndpoint {
    authority: String,
    host: String,
    port: u16,
    path: String,
}

impl ParsedEndpoint {
    fn parse(raw: &str) -> Result<Self, ModelError> {
        let rest = raw.strip_prefix("http://").ok_or_else(|| {
            ModelError::Endpoint("only http:// loopback endpoints are allowed".into())
        })?;
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if authority.is_empty() || authority.contains('@') {
            return Err(ModelError::Endpoint("invalid authority".into()));
        }
        let (host, port) = authority
            .rsplit_once(':')
            .map_or((authority, Ok(80_u16)), |(host, port)| {
                (host, port.parse::<u16>())
            });
        let port = port.map_err(|_| ModelError::Endpoint("invalid port".into()))?;
        if !matches!(host, "127.0.0.1" | "localhost") {
            return Err(ModelError::Endpoint(
                "host must be 127.0.0.1 or localhost".into(),
            ));
        }
        let path = format!("/{path}");
        if path.contains('?') || path.contains('#') || path == "/" {
            return Err(ModelError::Endpoint(
                "a fixed request path is required".into(),
            ));
        }
        Ok(Self {
            authority: authority.into(),
            host: host.into(),
            port,
            path,
        })
    }

    fn post(&self, body: &[u8]) -> Result<Vec<u8>, ModelError> {
        let address = (self.host.as_str(), self.port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| ModelError::Endpoint("loopback address did not resolve".into()))?;
        if !address.ip().is_loopback() {
            return Err(ModelError::Endpoint(
                "resolved endpoint is not loopback".into(),
            ));
        }
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))?;
        write!(
            stream,
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.path,
            self.authority,
            body.len()
        )?;
        stream.write_all(body)?;
        stream.flush()?;
        let mut response = Vec::new();
        let mut chunk = [0_u8; 8192];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    response.extend_from_slice(&chunk[..read]);
                    if response.len() as u64 > MAX_RESPONSE_BYTES {
                        return Err(ModelError::Response("response exceeds 1 MiB".into()));
                    }
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::ConnectionReset
                        && !response.is_empty() =>
                {
                    break;
                }
                Err(err) => return Err(ModelError::Io(err)),
            }
        }
        if response.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(ModelError::Response("response exceeds 1 MiB".into()));
        }
        parse_http_response(&response)
    }
}

fn parse_http_response(response: &[u8]) -> Result<Vec<u8>, ModelError> {
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| ModelError::Response("HTTP headers are incomplete".into()))?;
    let head = std::str::from_utf8(&response[..split])
        .map_err(|_| ModelError::Response("HTTP headers are not UTF-8".into()))?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| ModelError::Response("HTTP status is missing".into()))?;
    if !(200..300).contains(&status) {
        return Err(ModelError::Response(format!("HTTP status {status}")));
    }
    let mut content_length = None;
    let mut chunked = false;
    for line in head.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            return Err(ModelError::Response("malformed HTTP header".into()));
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| ModelError::Response("invalid Content-Length".into()))?,
            );
        }
        if name.eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
        {
            chunked = true;
        }
    }
    let body = &response[split + 4..];
    if chunked {
        return decode_chunked(body);
    }
    if let Some(expected) = content_length {
        if body.len() < expected {
            return Err(ModelError::Response(format!(
                "HTTP body is incomplete: expected {expected} bytes, received {}",
                body.len()
            )));
        }
        return Ok(body[..expected].to_vec());
    }
    Ok(body.to_vec())
}

fn decode_chunked(mut encoded: &[u8]) -> Result<Vec<u8>, ModelError> {
    let mut out = Vec::new();
    loop {
        let line_end = encoded
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(|| ModelError::Response("chunk size is incomplete".into()))?;
        let line = std::str::from_utf8(&encoded[..line_end])
            .map_err(|_| ModelError::Response("chunk size is not UTF-8".into()))?;
        let size = usize::from_str_radix(line.split(';').next().unwrap_or_default().trim(), 16)
            .map_err(|_| ModelError::Response("invalid chunk size".into()))?;
        encoded = &encoded[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if encoded.len() < size + 2 || &encoded[size..size + 2] != b"\r\n" {
            return Err(ModelError::Response("chunk body is incomplete".into()));
        }
        out.extend_from_slice(&encoded[..size]);
        if out.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(ModelError::Response("response exceeds 1 MiB".into()));
        }
        encoded = &encoded[size + 2..];
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(Deserialize)]
struct ChatUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
}
