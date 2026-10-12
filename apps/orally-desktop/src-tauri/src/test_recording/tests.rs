use super::*;
use orally_config::{PostprocessModelConfig, PromptNodeConfig};
use orally_core::AudioFormat;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::atomic::AtomicUsize;

const TEST_WAIT: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Counts {
    starts: AtomicUsize,
    stops: AtomicUsize,
    drops: AtomicUsize,
    bytes: AtomicUsize,
}

struct FakeRecorder {
    counts: Arc<Counts>,
    audio: AudioInput,
    // Like CPAL, this recorder cannot move between threads after construction.
    _thread_local: Rc<()>,
}

impl TestRecorder for FakeRecorder {
    fn byte_len(&self) -> Result<usize, String> {
        Ok(self.counts.bytes.load(Ordering::Acquire))
    }

    fn stop(self: Box<Self>) -> Result<AudioInput, String> {
        self.counts.stops.fetch_add(1, Ordering::AcqRel);
        Ok(self.audio.clone())
    }
}

impl Drop for FakeRecorder {
    fn drop(&mut self) {
        self.counts.drops.fetch_add(1, Ordering::AcqRel);
    }
}

fn audio(size: usize) -> AudioInput {
    AudioInput {
        bytes: vec![0; size],
        sample_rate_hz: 16_000,
        channels: 1,
        format: AudioFormat::Pcm16,
    }
}

fn factory(
    counts: Arc<Counts>,
    audio: AudioInput,
) -> impl FnMut() -> Result<Box<dyn TestRecorder>, String> + Send {
    move || {
        counts.starts.fetch_add(1, Ordering::AcqRel);
        Ok(Box::new(FakeRecorder {
            counts: counts.clone(),
            audio: audio.clone(),
            _thread_local: Rc::new(()),
        }))
    }
}

fn worker(counts: Arc<Counts>) -> RecordingWorker {
    RecordingWorker {
        gate: Arc::new(RecordingGate::default()),
        owner_epochs: Arc::new(OwnerEpochs::default()),
        factory: Box::new(factory(counts, audio(32))),
        limits: CaptureLimits::default(),
        active: None,
    }
}

fn start(worker: &mut RecordingWorker, config: AppConfig, asr_only: bool) -> String {
    worker
        .start(
            "main".to_string(),
            worker.owner_epochs.current("main"),
            config,
            asr_only,
        )
        .unwrap()
}

fn send_start(
    sender: &mpsc::Sender<TestCommand>,
    epochs: &OwnerEpochs,
    config: AppConfig,
    asr_only: bool,
) -> mpsc::Receiver<Result<String, String>> {
    let (reply, response) = mpsc::channel();
    sender
        .send(TestCommand::Start {
            owner: "main".to_string(),
            owner_epoch: epochs.current("main"),
            config: Box::new(config),
            asr_only,
            reply,
        })
        .unwrap();
    response
}

fn cancel(sender: &mpsc::Sender<TestCommand>, id: &str) {
    let (reply, response) = mpsc::channel();
    sender
        .send(TestCommand::Cancel {
            session_id: id.to_string(),
            reply,
        })
        .unwrap();
    response.recv_timeout(TEST_WAIT).unwrap();
}

#[test]
fn privacy_and_busy_dictation_prevent_microphone_start_and_factory_failure_releases_gate() {
    let counts = Arc::new(Counts::default());
    let mut worker = worker(counts.clone());
    let mut config = AppConfig::default();
    config.privacy.allow_external_requests = false;
    assert!(worker
        .start("main".to_string(), 0, config, false)
        .unwrap_err()
        .contains("隐私"));
    assert_eq!(counts.starts.load(Ordering::Acquire), 0);

    let dictation = worker.gate.acquire().unwrap();
    assert!(worker
        .start("main".to_string(), 0, AppConfig::default(), false)
        .unwrap_err()
        .contains("占用"));
    assert_eq!(counts.starts.load(Ordering::Acquire), 0);
    drop(dictation);
    worker.factory = Box::new(|| Err("麦克风录音启动失败：no input device".to_string()));
    assert!(worker
        .start("main".to_string(), 0, AppConfig::default(), false)
        .unwrap_err()
        .contains("no input device"));
    assert!(worker.gate.acquire().is_ok());
}

#[test]
fn processing_captures_draft_and_stale_session_cannot_stop_or_release_new_recording() {
    let counts = Arc::new(Counts::default());
    let mut worker = worker(counts.clone());
    let mut draft = AppConfig::default();
    draft.asr.model = "unsaved draft".to_string();
    let old_id = start(&mut worker, draft.clone(), true);
    draft.asr.model = "later edits".to_string();
    let input = worker.begin_processing(&old_id).unwrap();
    assert_eq!(input.config.asr.model, "unsaved draft");
    assert!(input.asr_only);
    assert_eq!(counts.stops.load(Ordering::Acquire), 1);
    assert_eq!(counts.drops.load(Ordering::Acquire), 1);
    assert!(
        worker.gate.acquire().is_err(),
        "processing still owns the dictation gate"
    );
    worker.cancel(&old_id);
    assert!(input.cancelled.load(Ordering::Acquire));
    let new_id = start(&mut worker, draft, false);
    assert_ne!(old_id, new_id);
    assert!(!worker.complete(&old_id));
    worker.cancel(&old_id);
    assert!(worker.begin_processing(&old_id).is_err());
    assert_eq!(worker.active.as_ref().unwrap().id, new_id);
    assert_eq!(counts.drops.load(Ordering::Acquire), 1);
    assert!(worker.gate.acquire().is_err());
    worker.cancel(&new_id);
    assert_eq!(counts.drops.load(Ordering::Acquire), 2);
    assert!(worker.gate.acquire().is_ok());
}

#[test]
fn released_old_lease_drop_cannot_release_a_new_owner() {
    let gate = Arc::new(RecordingGate::default());
    let old = gate.acquire().unwrap();
    old.release();
    let new = gate.acquire().unwrap();
    drop(old);
    assert!(gate.acquire().is_err());
    drop(new);
    assert!(gate.acquire().is_ok());
}

#[test]
fn duration_cap_stops_capture_once_and_retains_audio_until_stop_or_expiry() {
    let counts = Arc::new(Counts::default());
    let mut worker = worker(counts.clone());
    let id = start(&mut worker, AppConfig::default(), true);
    let started_at = match worker.active.as_ref().unwrap().state {
        CaptureState::Recording { started_at, .. } => started_at,
        _ => unreachable!(),
    };
    let now = started_at + worker.limits.duration;
    assert_eq!(
        worker.check_capture_limits(now).unwrap(),
        CaptureEvent {
            session_id: id.clone(),
            reason: "duration-limit".to_string(),
        }
    );
    assert_eq!(counts.stops.load(Ordering::Acquire), 1);
    assert_eq!(counts.drops.load(Ordering::Acquire), 1);
    assert!(worker.check_capture_limits(now).is_none());
    let input = worker.begin_processing(&id).unwrap();
    assert_eq!(input.audio, audio(32));
    assert!(input.warning.unwrap().contains("120 秒"));
    assert!(worker.complete(&id));
    assert!(worker.gate.acquire().is_ok());

    let id = start(&mut worker, AppConfig::default(), false);
    let now = Instant::now() + worker.limits.duration;
    worker.check_capture_limits(now).unwrap();
    let expired = worker
        .check_capture_limits(now + worker.limits.retained_duration)
        .unwrap();
    assert_eq!(expired.session_id, id);
    assert_eq!(expired.reason, "expired");
    assert!(worker.active.is_none());
    assert!(worker.gate.acquire().is_ok());
    assert!(worker.begin_processing(&id).is_err());
}

#[test]
fn size_cap_retains_complete_pcm_frames_and_owner_cancel_notifies_only_the_old_page() {
    let counts = Arc::new(Counts::default());
    counts.bytes.store(13, Ordering::Release);
    let mut worker = worker(counts.clone());
    let mut oversized = audio(24);
    oversized.channels = 2;
    worker.factory = Box::new(factory(counts, oversized));
    worker.limits.bytes = 13;
    let id = start(&mut worker, AppConfig::default(), false);
    assert_eq!(
        worker.check_capture_limits(Instant::now()).unwrap().reason,
        "size-limit"
    );
    let input = worker.begin_processing(&id).unwrap();
    assert_eq!(input.audio.bytes.len(), 12);
    assert!(input.warning.unwrap().contains("15 MiB"));
    let epoch = worker.owner_epochs.cancel("other");
    assert!(worker.cancel_owner("other", epoch).is_none());
    assert_eq!(worker.active.as_ref().unwrap().id, id);
    let epoch = worker.owner_epochs.cancel("main");
    assert_eq!(
        worker.cancel_owner("main", epoch).unwrap(),
        CaptureEvent {
            session_id: id,
            reason: "cancelled".to_string(),
        }
    );
    assert!(input.cancelled.load(Ordering::Acquire));
    assert!(worker.gate.acquire().is_ok());
    let new_id = start(&mut worker, AppConfig::default(), false);
    assert!(worker.cancel_owner("main", epoch).is_none());
    assert_eq!(worker.active.as_ref().unwrap().id, new_id);
    worker.cancel(&new_id);
}

#[test]
fn cancelled_pending_start_never_opens_microphone() {
    let counts = Arc::new(Counts::default());
    let mut worker = worker(counts.clone());
    let old_epoch = worker.owner_epochs.current("main");
    worker.owner_epochs.cancel("main");
    assert!(worker
        .start("main".to_string(), old_epoch, AppConfig::default(), false)
        .unwrap_err()
        .contains("页面"));
    assert_eq!(counts.starts.load(Ordering::Acquire), 0);
    assert!(worker.gate.acquire().is_ok());
}

#[test]
fn page_cancel_during_slow_factory_drops_late_stream_and_allows_new_start() {
    let gate = Arc::new(RecordingGate::default());
    let epochs = Arc::new(OwnerEpochs::default());
    let counts = Arc::new(Counts::default());
    let (entered, entering) = mpsc::channel();
    let (release, releasing) = mpsc::channel();
    let mut fake_factory = factory(counts.clone(), audio(32));
    let mut first = true;
    let (sender, thread) = spawn_worker(
        gate.clone(),
        epochs.clone(),
        move || {
            if first {
                first = false;
                entered.send(()).unwrap();
                releasing.recv_timeout(TEST_WAIT).unwrap();
            }
            fake_factory()
        },
        CaptureLimits::default(),
        Arc::new(|_| {}),
    )
    .unwrap();
    let old_response = send_start(&sender, &epochs, AppConfig::default(), true);
    entering.recv_timeout(TEST_WAIT).unwrap();
    let epoch = epochs.cancel("main");
    // Simulate another page enqueueing Start before the cancellation message.
    let new_response = send_start(&sender, &epochs, AppConfig::default(), false);
    sender
        .send(TestCommand::CancelOwner {
            owner: "main".to_string(),
            epoch,
        })
        .unwrap();
    release.send(()).unwrap();
    assert!(old_response
        .recv_timeout(TEST_WAIT)
        .unwrap()
        .unwrap_err()
        .contains("页面"));
    let new_id = new_response.recv_timeout(TEST_WAIT).unwrap().unwrap();
    assert_eq!(counts.starts.load(Ordering::Acquire), 2);
    assert_eq!(counts.drops.load(Ordering::Acquire), 1);
    assert!(gate.acquire().is_err());
    cancel(&sender, &new_id);
    assert_eq!(counts.drops.load(Ordering::Acquire), 2);
    assert!(gate.acquire().is_ok());
    sender.send(TestCommand::Shutdown).unwrap();
    thread.join().unwrap();
}

fn read_request(stream: &mut TcpStream) -> String {
    stream.set_nonblocking(false).unwrap();
    stream.set_read_timeout(Some(TEST_WAIT)).unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "request ended before its body was complete");
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(header_end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            if bytes.len() >= header_end + 4 + length {
                return String::from_utf8_lossy(&bytes).to_string();
            }
        }
    }
}

fn accept(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + TEST_WAIT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "expected a service request");
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("accept service request: {error}"),
        }
    }
}

fn respond(stream: &mut TcpStream, body: &str) {
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
}

fn listener() -> (String, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    (
        format!("http://{}/v1", listener.local_addr().unwrap()),
        listener,
    )
}

fn draft(endpoint: &str) -> AppConfig {
    let mut config = AppConfig::default();
    config.asr.base_url = endpoint.to_string();
    config.asr.model = "draft-asr".to_string();
    config.asr.api_key = Some("draft-asr-key".to_string());
    config.asr.api_key_env.clear();
    config.asr.protocol = "ignored-legacy-protocol".to_string();
    config.postprocess.mode = "llm".to_string();
    config.output.insert = true;
    config.privacy.history_enabled = true;
    config.privacy.history_path = Some("unusable history location\0".to_string());
    config
}

#[test]
fn asr_only_recording_uses_start_snapshot_and_skips_invalid_postprocess() {
    let (endpoint, listener) = listener();
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        let request = read_request(&mut stream);
        respond(&mut stream, "{\"text\":\"raw microphone text\"}");
        request
    });
    let counts = Arc::new(Counts::default());
    let mut worker = worker(counts);
    let mut config = draft(&endpoint);
    config.postprocess.base_url = "invalid URL".to_string();
    config.postprocess.api_key = None;
    config.postprocess.api_key_env.clear();
    let id = start(&mut worker, config.clone(), true);
    config.asr.model = "changed after start".to_string();
    config.asr.api_key = Some("changed-key".to_string());
    let input = worker.begin_processing(&id).unwrap();
    let result = process_input(input);
    let request = server.join().unwrap();
    let result = result.unwrap();
    assert!(request.starts_with("POST /v1/audio/transcriptions "));
    assert!(request.contains("authorization: Bearer draft-asr-key"));
    assert!(request.contains("draft-asr"));
    assert!(!request.contains("changed-key"));
    assert!(!request.contains("changed after start"));
    assert!(request.contains("RIFF"));
    assert_eq!(result.raw_transcript, "raw microphone text");
    assert_eq!(result.final_text, "raw microphone text");
    assert!(result.warnings.is_empty());
    assert!(worker.complete(&id));
}

#[test]
fn global_recording_test_runs_draft_model_and_prompts_then_returns_without_side_effects() {
    let (endpoint, listener) = listener();
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        for body in [
            "{\"text\":\"raw microphone text\"}",
            "{\"choices\":[{\"message\":{\"content\":\"polished microphone text\"}}]}",
        ] {
            let mut stream = accept(&listener);
            requests.push(read_request(&mut stream));
            respond(&mut stream, body);
        }
        requests
    });
    let counts = Arc::new(Counts::default());
    let mut worker = worker(counts);
    let mut config = draft(&endpoint);
    config.postprocess.models = vec![PostprocessModelConfig {
        id: "draft-model-node".to_string(),
        base_url: endpoint,
        model: "draft-model".to_string(),
        api_key: Some("draft-llm-key".to_string()),
        api_key_env: String::new(),
        system_prompt: "draft instructions".to_string(),
        user_template: "{{transcript}}".to_string(),
        fallback_to_builtin: false,
        prompts: vec![
            PromptNodeConfig {
                content: "first prompt".to_string(),
                ..PromptNodeConfig::default()
            },
            PromptNodeConfig {
                content: "skip this prompt".to_string(),
                enabled: false,
                ..PromptNodeConfig::default()
            },
            PromptNodeConfig {
                content: "second prompt".to_string(),
                ..PromptNodeConfig::default()
            },
        ],
        ..PostprocessModelConfig::default()
    }];
    let id = start(&mut worker, config, false);
    let input = worker.begin_processing(&id).unwrap();
    let result = process_input(input);
    let requests = server.join().unwrap();
    let result = result.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].starts_with("POST /v1/chat/completions "));
    assert!(requests[1].contains("authorization: Bearer draft-llm-key"));
    assert!(requests[1].contains("\"model\":\"draft-model\""));
    assert!(requests[1].contains("draft instructions\\n\\nfirst prompt\\n\\nsecond prompt"));
    assert!(!requests[1].contains("skip this prompt"));
    assert!(requests[1].contains("raw microphone text"));
    assert_eq!(result.raw_transcript, "raw microphone text");
    assert_eq!(result.final_text, "polished microphone text");
    assert!(worker.complete(&id));
}

#[test]
fn network_processing_does_not_block_cancel_and_new_capture_or_accept_old_result() {
    let (endpoint, listener) = listener();
    let (received, request_received) = mpsc::channel();
    let (release, releasing) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        let request = read_request(&mut stream);
        received.send(()).unwrap();
        releasing.recv_timeout(TEST_WAIT).unwrap();
        respond(&mut stream, "{\"text\":\"old response\"}");
        request
    });
    let counts = Arc::new(Counts::default());
    let gate = Arc::new(RecordingGate::default());
    let epochs = Arc::new(OwnerEpochs::default());
    let (sender, thread) = spawn_worker(
        gate.clone(),
        epochs.clone(),
        factory(counts.clone(), audio(32)),
        CaptureLimits::default(),
        Arc::new(|_| {}),
    )
    .unwrap();
    let old_id = send_start(&sender, &epochs, draft(&endpoint), true)
        .recv_timeout(TEST_WAIT)
        .unwrap()
        .unwrap();
    let (reply, result) = mpsc::channel();
    sender
        .send(TestCommand::Stop {
            session_id: old_id.clone(),
            reply,
        })
        .unwrap();
    request_received.recv_timeout(TEST_WAIT).unwrap();
    assert_eq!(counts.stops.load(Ordering::Acquire), 1);
    assert_eq!(counts.drops.load(Ordering::Acquire), 1);
    assert!(gate.acquire().is_err());
    cancel(&sender, &old_id);
    let new_id = send_start(&sender, &epochs, AppConfig::default(), false)
        .recv_timeout(TEST_WAIT)
        .unwrap()
        .unwrap();
    assert_ne!(new_id, old_id);
    release.send(()).unwrap();
    assert!(result
        .recv_timeout(TEST_WAIT)
        .unwrap()
        .unwrap_err()
        .contains("取消"));
    server.join().unwrap();
    cancel(&sender, &old_id);
    assert_eq!(counts.drops.load(Ordering::Acquire), 1);
    assert!(gate.acquire().is_err());
    sender.send(TestCommand::Shutdown).unwrap();
    thread.join().unwrap();
    assert_eq!(counts.drops.load(Ordering::Acquire), 2);
    assert!(gate.acquire().is_ok());
}

#[test]
fn channel_worker_auto_capture_event_can_be_followed_by_stop_and_releases_gate() {
    let (endpoint, listener) = listener();
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        read_request(&mut stream);
        respond(&mut stream, "{\"text\":\"auto captured audio\"}");
    });
    let counts = Arc::new(Counts::default());
    let gate = Arc::new(RecordingGate::default());
    let epochs = Arc::new(OwnerEpochs::default());
    let (events, received_events) = mpsc::channel();
    let limits = CaptureLimits {
        duration: Duration::from_millis(20),
        retained_duration: TEST_WAIT,
        ..CaptureLimits::default()
    };
    let (sender, thread) = spawn_worker(
        gate.clone(),
        epochs.clone(),
        factory(counts.clone(), audio(32)),
        limits,
        Arc::new(move |event| {
            events.send(event).unwrap();
        }),
    )
    .unwrap();
    let id = send_start(&sender, &epochs, draft(&endpoint), true)
        .recv_timeout(TEST_WAIT)
        .unwrap()
        .unwrap();
    assert_eq!(
        received_events.recv_timeout(TEST_WAIT).unwrap(),
        CaptureEvent {
            session_id: id.clone(),
            reason: "duration-limit".to_string(),
        }
    );
    assert_eq!(counts.stops.load(Ordering::Acquire), 1);
    assert_eq!(counts.drops.load(Ordering::Acquire), 1);
    let (reply, response) = mpsc::channel();
    sender
        .send(TestCommand::Stop {
            session_id: id,
            reply,
        })
        .unwrap();
    let result = response.recv_timeout(TEST_WAIT).unwrap().unwrap();
    assert_eq!(result.final_text, "auto captured audio");
    assert_eq!(result.warnings.len(), 1);
    assert!(gate.acquire().is_ok());
    assert_eq!(counts.stops.load(Ordering::Acquire), 1);
    server.join().unwrap();
    sender.send(TestCommand::Shutdown).unwrap();
    thread.join().unwrap();
}
