use super::*;
use std::net::{Shutdown, TcpStream};

struct Reply {
    status: &'static str,
    body: &'static str,
    early: bool,
    truncated: bool,
}

impl Reply {
    fn complete(status: &'static str, body: &'static str) -> Self {
        Self {
            status,
            body,
            early: false,
            truncated: false,
        }
    }
}

fn read_headers(stream: &mut TcpStream) -> Vec<u8> {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "request must contain complete headers");
        request.extend_from_slice(&buffer[..count]);
        if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            return request;
        }
    }
}

fn serve_early_responses(replies: Vec<Reply>) -> (String, thread::JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        // Keep the read half open after sending the response and FIN. Dropping it
        // with unread upload bytes would introduce an unrelated TCP reset race.
        let mut rejected_streams = Vec::new();
        loop {
            let deadline = Instant::now()
                + if requests.len() < replies.len() {
                    Duration::from_secs(5)
                } else {
                    Duration::from_millis(400)
                };
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return requests;
                        }
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("accept local request: {error}"),
                }
            };
            let reply = replies
                .get(requests.len())
                .expect("must not negotiate an unauthorized request format");
            requests.push(if reply.early {
                read_headers(&mut stream)
            } else {
                read_request(&mut stream)
            });
            let length = reply.body.len() + usize::from(reply.truncated);
            write!(
                stream,
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{}",
                reply.status, reply.body
            )
            .unwrap();
            stream.flush().unwrap();
            stream.shutdown(Shutdown::Write).unwrap();
            rejected_streams.push(stream);
        }
    });
    (base_url, server)
}

#[test]
fn early_http_route_rejection_of_large_audio_reaches_chat() {
    let audio = AudioInput {
        bytes: vec![0; 48_000 * 2 * 2 * 60],
        sample_rate_hz: 48_000,
        channels: 2,
        format: AudioFormat::Pcm16,
    };
    for status in ["404 Not Found", "405 Method Not Allowed"] {
        let (base_url, server) = serve_early_responses(vec![
            Reply {
                early: true,
                ..Reply::complete(status, "{\"error\":\"route unavailable\"}")
            },
            Reply::complete(
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"heard\"}}]}",
            ),
        ]);
        let result = auto_provider(base_url).transcribe(audio.clone());
        let requests = server.join().unwrap();
        assert_eq!(
            result
                .unwrap_or_else(|error| panic!("early {status} must reach Chat Audio: {error}"))
                .text,
            "heard"
        );
        assert_eq!(requests.len(), 2);
        assert!(requests[0].len() < audio.bytes.len());
        assert!(String::from_utf8_lossy(&requests[0]).starts_with("POST /v1/audio/transcriptions "));
        assert_eq!(data_url_audio(&requests[1]), audio);
    }
}

#[test]
fn truncated_http_error_body_retains_status_and_only_route_errors_negotiate() {
    for status in [
        "404 Not Found",
        "405 Method Not Allowed",
        "400 Bad Request",
        "401 Unauthorized",
        "403 Forbidden",
        "422 Unprocessable Entity",
        "429 Too Many Requests",
        "500 Internal Server Error",
        "503 Service Unavailable",
    ] {
        let route_missing = status.starts_with("404") || status.starts_with("405");
        let mut replies = vec![Reply {
            truncated: true,
            ..Reply::complete(status, "{\"error\":\"rejected\"}")
        }];
        if route_missing {
            replies.push(Reply::complete(
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"heard\"}}]}",
            ));
        }
        let (base_url, server) = serve_early_responses(replies);
        let result = auto_provider(base_url).transcribe(fixture_audio());
        let requests = server.join().unwrap();
        if route_missing {
            assert_eq!(result.unwrap().text, "heard");
            assert_eq!(requests.len(), 2);
        } else {
            assert!(result.unwrap_err().to_string().contains(status));
            assert_eq!(requests.len(), 1);
        }
    }
}

#[test]
fn truncated_chat_shape_error_body_still_negotiates_standard_audio() {
    for status in ["400 Bad Request", "422 Unprocessable Entity"] {
        let (base_url, server) = serve_early_responses(vec![
            Reply::complete("404 Not Found", "{}"),
            Reply {
                truncated: true,
                ..Reply::complete(status, "{\"error\":\"audio format\"}")
            },
            Reply::complete(
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"heard\"}}]}",
            ),
        ]);
        let result = auto_provider(base_url).transcribe(fixture_audio());
        let requests = server.join().unwrap();
        assert_eq!(result.unwrap().text, "heard");
        assert_eq!(requests.len(), 3);
        assert_eq!(standard_chat_audio(&requests[2]), fixture_audio());
    }
}

#[test]
fn truncated_success_response_does_not_negotiate_another_format() {
    let (base_url, server) = serve_early_responses(vec![Reply {
        truncated: true,
        ..Reply::complete("200 OK", "{\"text\":\"heard\"}")
    }]);
    let result = auto_provider(base_url).transcribe(fixture_audio());
    let requests = server.join().unwrap();
    assert!(result.is_err());
    assert_eq!(requests.len(), 1);
}

#[test]
fn request_timeout_without_http_status_does_not_negotiate_another_format() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        thread::sleep(Duration::from_millis(200));
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "a timeout must not trigger a Chat Audio request"
        );
    });
    let mut config = OpenAiCompatibleAsrConfig::new(base_url, "fixture-key", "audio-transcriber");
    config.timeout = Duration::from_millis(50);
    let result = AutoAsrProvider::new(config)
        .unwrap()
        .transcribe(fixture_audio());
    server.join().unwrap();
    assert!(result.is_err());
}
