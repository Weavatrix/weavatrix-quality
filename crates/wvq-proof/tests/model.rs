use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use wvq_proof::{
    AiBudget, AiCallKind, AiCostFirewall, LocalModelConfig, LocalModelRequest, call_local_model,
};

#[test]
fn loopback_model_usage_is_measured_and_charged() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        let request = String::from_utf8_lossy(&request);
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(request.contains("\"model\":\"local-test\""));
        let body = br#"{"model":"local-test","choices":[{"message":{"content":"probe the boundary"}}],"usage":{"prompt_tokens":3,"completion_tokens":2}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:X}\r\n",
            body.len(),
        )
        .unwrap();
        stream.write_all(body).unwrap();
        stream.write_all(b"\r\n0\r\n\r\n").unwrap();
    });
    let mut firewall = AiCostFirewall::new(AiBudget {
        runtime_tokens: 100,
        browser_escape_calls: 1,
        max_cost_micros: Some(100),
        ..AiBudget::default()
    });
    let reply = call_local_model(
        &LocalModelConfig {
            endpoint: format!("http://{address}/v1/chat/completions"),
            model: "local-test".into(),
            max_output_tokens: 8,
            input_micros_per_million: 1_000_000,
            output_micros_per_million: 2_000_000,
        },
        &LocalModelRequest {
            kind: AiCallKind::BrowserEscape,
            prompt: "probe".into(),
        },
        &mut firewall,
    )
    .unwrap();
    server.join().unwrap();

    assert_eq!(reply.text, "probe the boundary");
    assert_eq!(reply.input_tokens, 3);
    assert_eq!(reply.output_tokens, 2);
    assert_eq!(reply.cost_micros, 7);
    assert_eq!(firewall.usage().runtime_tokens, 5);
    assert_eq!(firewall.usage().browser_escape_calls, 1);
}

fn read_http_request(stream: &mut impl Read) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    let mut expected = None;
    loop {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0, "request closed before its body was complete");
        request.extend_from_slice(&buffer[..read]);
        if expected.is_none()
            && let Some(split) = request.windows(4).position(|window| window == b"\r\n\r\n")
        {
            let head = String::from_utf8_lossy(&request[..split]);
            let content_length = head
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap();
            expected = Some(split + 4 + content_length);
        }
        if expected.is_some_and(|expected| request.len() >= expected) {
            return request;
        }
    }
}

#[test]
fn non_loopback_endpoint_is_refused_before_network_io() {
    let mut firewall = AiCostFirewall::new(AiBudget {
        runtime_tokens: 100,
        ..AiBudget::default()
    });
    let error = call_local_model(
        &LocalModelConfig {
            endpoint: "http://example.com/v1/chat/completions".into(),
            model: "forbidden".into(),
            max_output_tokens: 8,
            input_micros_per_million: 0,
            output_micros_per_million: 0,
        },
        &LocalModelRequest {
            kind: AiCallKind::Runtime,
            prompt: "secret".into(),
        },
        &mut firewall,
    )
    .unwrap_err();
    assert!(error.to_string().contains("host must be"));
    assert_eq!(firewall.usage().runtime_tokens, 0);
}
