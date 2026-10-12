use super::*;
use chrono::TimeZone;
use std::io::Read;
use std::net::TcpListener;
use std::time::Duration;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "orally-diagnostics-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            REQUEST_ID.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&directory).unwrap();
        Self(directory)
    }

    fn logger(&self, auto_cleanup: bool, retention_days: u16) -> Logger {
        Logger {
            directory: self.0.clone(),
            auto_cleanup,
            retention_days,
            cleaned: None,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn at(day: u32) -> chrono::DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 10, day, 12, 0, 0)
        .single()
        .unwrap()
}

#[test]
fn append_and_restart_preserve_today_then_idle_midnight_cleanup_removes_yesterday() {
    let fixture = Fixture::new();
    let mut logger = fixture.logger(true, 1);
    logger.append(at(10), &Event::SessionStarted).unwrap();
    let mut restarted = fixture.logger(true, 1);
    restarted.append(at(10), &Event::SessionStarted).unwrap();
    let yesterday = fixture.0.join("orally-2026-10-10.jsonl");
    assert_eq!(fs::read_to_string(&yesterday).unwrap().lines().count(), 2);
    // Idle maintenance alone must remove stale logs; no request is necessary.
    restarted.maintain(at(11).date_naive()).unwrap();
    assert!(!yesterday.exists());
    restarted.append(at(11), &Event::SessionStarted).unwrap();
    assert_eq!(
        fs::read_to_string(fixture.0.join("orally-2026-10-11.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn retention_uses_calendar_days_and_preserves_unrelated_files_and_directories() {
    let fixture = Fixture::new();
    for file in [
        "orally-2026-10-08.jsonl",
        "orally-2026-10-09.jsonl",
        "orally-2026-10-10.jsonl",
        "history.jsonl",
        "orally-invalid.jsonl",
        "orally-2026-10-08.jsonl.bak",
        "orally-2026-10-12.jsonl",
    ] {
        fs::write(fixture.0.join(file), "{}").unwrap();
    }
    fs::create_dir(fixture.0.join("orally-2026-10-01.jsonl")).unwrap();
    let mut logger = fixture.logger(true, 3);
    logger.maintain(at(11).date_naive()).unwrap();
    assert!(!fixture.0.join("orally-2026-10-08.jsonl").exists());
    for name in [
        "orally-2026-10-09.jsonl",
        "orally-2026-10-10.jsonl",
        "history.jsonl",
        "orally-invalid.jsonl",
        "orally-2026-10-08.jsonl.bak",
        "orally-2026-10-12.jsonl",
        "orally-2026-10-01.jsonl",
    ] {
        assert!(fixture.0.join(name).exists(), "{name}");
    }
}

#[test]
fn disabled_cleanup_keeps_daily_files_and_reenable_removes_expired_files() {
    let fixture = Fixture::new();
    let mut logger = fixture.logger(false, 1);
    logger.append(at(9), &Event::SessionStarted).unwrap();
    logger.append(at(11), &Event::SessionStarted).unwrap();
    assert!(fixture.0.join("orally-2026-10-09.jsonl").exists());
    let mut enabled = fixture.logger(true, 1);
    enabled.maintain(at(11).date_naive()).unwrap();
    assert!(!fixture.0.join("orally-2026-10-09.jsonl").exists());
    assert!(fixture.0.join("orally-2026-10-11.jsonl").exists());
}

#[test]
fn network_traces_keep_status_and_failure_categories_without_secrets_or_content() {
    let fixture = Fixture::new();
    configure(fixture.0.clone(), false, 1).unwrap();
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    for (status, body, outcome) in [
        ("200 OK", r#"{"text":"private-return-content"}"#, "success"),
        (
            "429 Too Many Requests",
            "private-error-content",
            "http_error",
        ),
        ("200 OK", "private-invalid-content", "invalid_response"),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/secret-path?token=private-query",
            listener.local_addr().unwrap()
        );
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 2048];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let len = headers
                        .lines()
                        .find_map(|s| {
                            s.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nX-Secret: private-header\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let request = client
            .post(url)
            .bearer_auth("private-key")
            .body("private-audio-and-prompt")
            .build()
            .unwrap();
        let mut trace = HttpTrace::new(Protocol::ChatText);
        let response = trace.send(&client, request).unwrap();
        if response.status().is_success() {
            if trace.json::<serde_json::Value>(response).is_ok() {
                trace.success();
            }
        } else {
            assert_eq!(trace.error_text(response).unwrap(), body);
        }
        drop(trace);
        server.join().unwrap();
        let log = fs::read_to_string(
            fixture
                .0
                .join(format!("orally-{}.jsonl", Local::now().format("%Y-%m-%d"))),
        )
        .unwrap();
        let event: serde_json::Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
        assert_eq!(event["event"]["outcome"], outcome);
        assert_eq!(
            event["event"]["status"],
            status[..3].parse::<u16>().unwrap()
        );
    }
    // A transport failure contains a credential-bearing URL in reqwest's error,
    // but only its category may reach the log.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let mut trace = HttpTrace::new(Protocol::Transcriptions);
    assert!(trace
        .send(
            &client,
            client
                .post(format!("http://{address}/?secret=private-query"))
                .build()
                .unwrap()
        )
        .is_err());
    drop(trace);
    recording(1234, 39488, 16000, 1);
    *LOGGER.lock().unwrap() = None;
    let log = fs::read_dir(&fixture.0)
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<String>();
    for secret in [
        "private-",
        "secret-path",
        "token=",
        "authorization",
        "127.0.0.1",
    ] {
        assert!(!log.contains(secret), "leaked {secret}");
    }
    let entries = log
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    // Windows can retry a refused connection until reqwest's deadline expires.
    assert!(entries.iter().any(|e| matches!(
        e["event"]["outcome"].as_str(),
        Some("connection" | "timeout")
    )));
    assert!(entries.iter().any(|e| e["event"]["duration_ms"] == 1234));
    assert!(entries.iter().all(|e| e["level"] == "DEBUG"));
}
