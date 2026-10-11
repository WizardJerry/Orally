//! Allowlisted diagnostic metadata only. Never accept arbitrary messages or bodies.
use chrono::{Local, NaiveDate};
use reqwest::blocking::{Client, Request, Response};
use serde::{de::DeserializeOwned, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

static LOGGER: Mutex<Option<Logger>> = Mutex::new(None);
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

struct Logger {
    directory: PathBuf,
    auto_cleanup: bool,
    retention_days: u16,
    cleaned: Option<NaiveDate>,
}

impl Logger {
    fn maintain(&mut self, today: NaiveDate) -> io::Result<()> {
        fs::create_dir_all(&self.directory)?;
        if self.auto_cleanup && self.cleaned != Some(today) {
            for entry in fs::read_dir(&self.directory)? {
                let entry = entry?;
                // Never follow links or remove unrelated files/directories.
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let name = entry.file_name();
                let Some(date) = name
                    .to_str()
                    .and_then(|s| s.strip_prefix("orally-"))
                    .and_then(|s| s.strip_suffix(".jsonl"))
                    .filter(|s| s.len() == 10)
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                else {
                    continue;
                };
                if today.signed_duration_since(date).num_days()
                    >= i64::from(self.retention_days.max(1))
                {
                    fs::remove_file(entry.path())?;
                }
            }
            self.cleaned = Some(today);
        }
        Ok(())
    }

    fn append(&mut self, now: chrono::DateTime<Local>, event: &Event) -> io::Result<()> {
        self.maintain(now.date_naive())?;
        let path = self
            .directory
            .join(format!("orally-{}.jsonl", now.format("%Y-%m-%d")));
        let mut line = serde_json::to_vec(&serde_json::json!({
            "timestamp": now.to_rfc3339(), "level": "DEBUG", "pid": std::process::id(), "event": event,
        }))?;
        line.push(b'\n');
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(&line)
    }
}

/// Configure the process sink. Failures must not prevent speech input.
pub fn configure(directory: PathBuf, auto_cleanup: bool, retention_days: u16) -> io::Result<()> {
    let mut state = LOGGER.lock().unwrap_or_else(|e| e.into_inner());
    if state.as_ref().is_some_and(|l| {
        l.directory == directory
            && l.auto_cleanup == auto_cleanup
            && l.retention_days == retention_days
    }) {
        return Ok(());
    }
    let mut logger = Logger {
        directory,
        auto_cleanup,
        retention_days,
        cleaned: None,
    };
    let result = logger.maintain(Local::now().date_naive());
    *state = Some(logger);
    result
}

/// Called by the desktop's idle maintenance timer, including when no requests occur.
pub fn maintain() -> io::Result<()> {
    if let Some(logger) = LOGGER.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        logger.maintain(Local::now().date_naive())?;
    }
    Ok(())
}

fn emit(event: Event) {
    if let Some(logger) = LOGGER.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        if logger.append(Local::now(), &event).is_err() {
            eprintln!("Orally diagnostic log write failed");
        }
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Transcriptions,
    ChatAudioDataUrl,
    ChatAudioBase64,
    ChatText,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Pending,
    Success,
    HttpError,
    Timeout,
    Connection,
    Transport,
    ResponseRead,
    InvalidResponse,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    SessionStarted,
    Recording {
        duration_ms: u64,
        bytes: usize,
        sample_rate_hz: u32,
        channels: u16,
    },
    RequestStarted {
        request_id: u64,
        protocol: Protocol,
        method: &'static str,
        request_bytes: Option<usize>,
    },
    RequestFinished {
        request_id: u64,
        protocol: Protocol,
        status: Option<u16>,
        response_content_length: Option<u64>,
        elapsed_ms: u128,
        outcome: Outcome,
    },
}

pub fn recording(duration_ms: u64, bytes: usize, sample_rate_hz: u32, channels: u16) {
    emit(Event::Recording {
        duration_ms,
        bytes,
        sample_rate_hz,
        channels,
    });
}

pub fn session_started() {
    emit(Event::SessionStarted);
}

/// One trace per actual HTTP attempt, including protocol fallback attempts.
/// Drop reports early failures without ever formatting provider errors.
pub struct HttpTrace {
    id: u64,
    protocol: Protocol,
    started: Instant,
    status: Option<u16>,
    response_content_length: Option<u64>,
    outcome: Outcome,
}

impl HttpTrace {
    pub fn new(protocol: Protocol) -> Self {
        Self {
            id: REQUEST_ID.fetch_add(1, Ordering::Relaxed),
            protocol,
            started: Instant::now(),
            status: None,
            response_content_length: None,
            outcome: Outcome::Pending,
        }
    }

    pub fn send(&mut self, client: &Client, request: Request) -> Result<Response, reqwest::Error> {
        emit(Event::RequestStarted {
            request_id: self.id,
            protocol: self.protocol,
            method: "POST",
            request_bytes: request
                .body()
                .and_then(|body| body.as_bytes())
                .map(<[u8]>::len),
        });
        let result = client.execute(request);
        match &result {
            Ok(response) => {
                self.status = Some(response.status().as_u16());
                self.response_content_length = response.content_length();
                self.outcome = if response.status().is_success() {
                    Outcome::InvalidResponse
                } else {
                    Outcome::HttpError
                };
            }
            Err(error) => {
                self.outcome = if error.is_timeout() {
                    Outcome::Timeout
                } else if error.is_connect() {
                    Outcome::Connection
                } else {
                    Outcome::Transport
                }
            }
        }
        result
    }

    pub fn json<T: DeserializeOwned>(&mut self, response: Response) -> Result<T, reqwest::Error> {
        let result = response.json();
        if let Err(error) = &result {
            self.outcome = if error.is_timeout() {
                Outcome::Timeout
            } else if error.is_decode() {
                Outcome::InvalidResponse
            } else {
                Outcome::ResponseRead
            };
        }
        result
    }

    pub fn error_text(&mut self, response: Response) -> Result<String, reqwest::Error> {
        let result = response.text();
        if result.is_err() {
            self.outcome = Outcome::ResponseRead;
        }
        result
    }

    pub fn success(&mut self) {
        self.outcome = Outcome::Success;
    }
}

impl Drop for HttpTrace {
    fn drop(&mut self) {
        emit(Event::RequestFinished {
            request_id: self.id,
            protocol: self.protocol,
            status: self.status,
            response_content_length: self.response_content_length,
            elapsed_ms: self.started.elapsed().as_millis(),
            outcome: self.outcome,
        });
    }
}

#[cfg(test)]
mod tests;
