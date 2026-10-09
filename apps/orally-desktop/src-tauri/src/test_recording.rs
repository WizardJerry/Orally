//! Microphone tests own their non-Send CPAL stream on one recording thread.

use super::service_test::{self, InputTestResult};
use orally_audio::CpalRecordingSession;
use orally_config::AppConfig;
use orally_core::AudioInput;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

const CAPTURE_EVENT: &str = "test-recording-captured";
const WORKER_POLL: Duration = Duration::from_millis(100);
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

pub(super) struct RecordingGate {
    owner: AtomicU64,
    next_token: AtomicU64,
}

impl Default for RecordingGate {
    fn default() -> Self {
        Self {
            owner: AtomicU64::new(0),
            next_token: AtomicU64::new(1),
        }
    }
}

impl RecordingGate {
    pub(super) fn acquire(self: &Arc<Self>) -> Result<RecordingLease, String> {
        let token = self
            .next_token
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |token| {
                token.checked_add(1)
            })
            .map_err(|_| "无法开始新的录音，请重启 Orally 后重试。".to_string())?;
        self.owner
            .compare_exchange(0, token, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "麦克风已被语音输入或其他测试占用，请先完成当前任务。".to_string())?;
        Ok(RecordingLease {
            gate: self.clone(),
            token,
        })
    }
}

pub(super) struct RecordingLease {
    gate: Arc<RecordingGate>,
    token: u64,
}

impl RecordingLease {
    fn release(&self) {
        let _ =
            self.gate
                .owner
                .compare_exchange(self.token, 0, Ordering::AcqRel, Ordering::Acquire);
    }
}

impl Drop for RecordingLease {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Clone, Copy)]
struct CaptureLimits {
    duration: Duration,
    retained_duration: Duration,
    bytes: usize,
}

impl Default for CaptureLimits {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(120),
            retained_duration: Duration::from_secs(30),
            // Leave room for the WAV header added by the recognition adapter.
            bytes: 15 * 1024 * 1024 - 44,
        }
    }
}

trait TestRecorder {
    fn byte_len(&self) -> Result<usize, String>;
    fn stop(self: Box<Self>) -> Result<AudioInput, String>;
}

impl TestRecorder for CpalRecordingSession {
    fn byte_len(&self) -> Result<usize, String> {
        self.captured_byte_len()
            .map_err(|error| format!("麦克风录音状态读取失败：{error}"))
    }

    fn stop(self: Box<Self>) -> Result<AudioInput, String> {
        (*self)
            .stop()
            .map(|recorded| recorded.audio)
            .map_err(|error| format!("麦克风录音停止失败：{error}"))
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct CaptureEvent {
    session_id: String,
    reason: String,
}

enum CaptureState {
    Recording {
        recorder: Box<dyn TestRecorder>,
        started_at: Instant,
    },
    Captured {
        audio: Result<AudioInput, String>,
        captured_at: Instant,
        warning: Option<String>,
    },
    Processing {
        cancelled: Arc<AtomicBool>,
    },
}

struct TestSession {
    id: String,
    owner: String,
    owner_epoch: u64,
    config: AppConfig,
    asr_only: bool,
    _lease: RecordingLease,
    state: CaptureState,
}

struct ProcessingInput {
    config: AppConfig,
    asr_only: bool,
    audio: AudioInput,
    warning: Option<String>,
    cancelled: Arc<AtomicBool>,
}

type RecorderFactory = dyn FnMut() -> Result<Box<dyn TestRecorder>, String>;

#[derive(Default)]
struct OwnerEpochs(Mutex<HashMap<String, u64>>);

impl OwnerEpochs {
    fn current(&self, owner: &str) -> u64 {
        *self
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry(owner.to_string())
            .or_default()
    }

    fn cancel(&self, owner: &str) -> u64 {
        let mut epochs = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let epoch = epochs.entry(owner.to_string()).or_default();
        *epoch = epoch.saturating_add(1);
        *epoch
    }
}

struct RecordingWorker {
    gate: Arc<RecordingGate>,
    owner_epochs: Arc<OwnerEpochs>,
    factory: Box<RecorderFactory>,
    limits: CaptureLimits,
    active: Option<TestSession>,
}

impl RecordingWorker {
    fn start(
        &mut self,
        owner: String,
        owner_epoch: u64,
        config: AppConfig,
        asr_only: bool,
    ) -> Result<String, String> {
        service_test::require_external_requests(&config)?;
        if self.owner_epochs.current(&owner) != owner_epoch {
            return Err("页面已关闭或重新加载，已取消录音启动。".to_string());
        }
        if self.active.is_some() {
            return Err("已有测试录音或测试处理正在进行，请先停止或取消。".to_string());
        }
        let lease = self.gate.acquire()?;
        let recorder = (self.factory)()?;
        if self.owner_epochs.current(&owner) != owner_epoch {
            return Err("页面已关闭或重新加载，已取消录音启动。".to_string());
        }
        let id = format!(
            "test-{}-{}",
            std::process::id(),
            NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed)
        );
        self.active = Some(TestSession {
            id: id.clone(),
            owner,
            owner_epoch,
            config,
            asr_only,
            _lease: lease,
            state: CaptureState::Recording {
                recorder,
                started_at: Instant::now(),
            },
        });
        Ok(id)
    }

    fn begin_processing(&mut self, id: &str) -> Result<ProcessingInput, String> {
        let session = self
            .active
            .as_ref()
            .filter(|session| session.id == id)
            .ok_or_else(|| "录音会话已结束或已失效，请重新录音。".to_string())?;
        if matches!(session.state, CaptureState::Processing { .. }) {
            return Err("这段测试录音正在处理，请等待结果或取消。".to_string());
        }
        let mut session = self.active.take().unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let capture = std::mem::replace(
            &mut session.state,
            CaptureState::Processing {
                cancelled: cancelled.clone(),
            },
        );
        let (audio, warning) = match capture {
            CaptureState::Recording { recorder, .. } => (recorder.stop()?, None),
            CaptureState::Captured { audio, warning, .. } => (audio?, warning),
            CaptureState::Processing { .. } => unreachable!("processing was checked above"),
        };
        if audio.bytes.is_empty() {
            return Err("麦克风未采集到音频，请检查输入设备后重新录音。".to_string());
        }
        if audio.bytes.len() > self.limits.bytes {
            return Err("测试录音不能超过 15 MiB，请缩短录音后重试。".to_string());
        }
        let input = ProcessingInput {
            config: session.config.clone(),
            asr_only: session.asr_only,
            audio,
            warning,
            cancelled,
        };
        self.active = Some(session);
        Ok(input)
    }

    fn complete(&mut self, id: &str) -> bool {
        let Some(session) = self.active.as_ref().filter(|session| session.id == id) else {
            return false;
        };
        let CaptureState::Processing { cancelled } = &session.state else {
            return false;
        };
        let accept_result = !cancelled.load(Ordering::Acquire);
        self.active.take();
        accept_result
    }

    fn cancel(&mut self, id: &str) {
        if self.active.as_ref().is_some_and(|session| session.id == id) {
            let session = self.active.take().unwrap();
            if let CaptureState::Processing { cancelled } = &session.state {
                cancelled.store(true, Ordering::Release);
            }
            // Dropping a capture closes CPAL; dropping the token releases the gate.
            drop(session);
        }
    }

    fn cancel_owner(&mut self, owner: &str, epoch: u64) -> Option<CaptureEvent> {
        if let Some(id) = self
            .active
            .as_ref()
            .filter(|session| session.owner == owner && session.owner_epoch < epoch)
            .map(|session| session.id.clone())
        {
            self.cancel(&id);
            return Some(CaptureEvent {
                session_id: id,
                reason: "cancelled".to_string(),
            });
        }
        None
    }

    fn check_capture_limits(&mut self, now: Instant) -> Option<CaptureEvent> {
        let session = self.active.as_ref()?;
        let reason = match &session.state {
            CaptureState::Recording {
                recorder,
                started_at,
            } => {
                if now.saturating_duration_since(*started_at) >= self.limits.duration {
                    "duration-limit"
                } else {
                    match recorder.byte_len() {
                        Ok(bytes) if bytes >= self.limits.bytes => "size-limit",
                        Ok(_) => return None,
                        Err(_) => "recording-error",
                    }
                }
            }
            CaptureState::Captured { captured_at, .. }
                if now.saturating_duration_since(*captured_at) >= self.limits.retained_duration =>
            {
                "expired"
            }
            CaptureState::Captured { .. } | CaptureState::Processing { .. } => return None,
        };
        let id = session.id.clone();
        if reason == "expired" {
            self.cancel(&id);
        } else {
            let session = self.active.as_mut().unwrap();
            let capture = std::mem::replace(
                &mut session.state,
                CaptureState::Captured {
                    audio: Err("麦克风录音状态不可用，请重新录音。".to_string()),
                    captured_at: now,
                    warning: None,
                },
            );
            let CaptureState::Recording { recorder, .. } = capture else {
                unreachable!("only recording sessions are captured");
            };
            let audio = recorder.stop().map(|mut audio| {
                if audio.bytes.len() > self.limits.bytes {
                    let frame_bytes = usize::from(audio.channels.max(1)) * 2;
                    audio
                        .bytes
                        .truncate(self.limits.bytes / frame_bytes * frame_bytes);
                }
                audio
            });
            let warning = match reason {
                "duration-limit" => "录音已达到 120 秒上限，已自动停止采集。",
                "size-limit" => "录音已达到 15 MiB 上限，已自动停止并保留上限内的音频。",
                _ => "麦克风录音状态读取失败，已停止采集。",
            };
            session.state = CaptureState::Captured {
                audio,
                captured_at: now,
                warning: Some(warning.to_string()),
            };
        }
        Some(CaptureEvent {
            session_id: id,
            reason: reason.to_string(),
        })
    }
}

type StartReply = mpsc::Sender<Result<String, String>>;
type StopReply = mpsc::Sender<Result<InputTestResult, String>>;

enum TestCommand {
    Start {
        owner: String,
        owner_epoch: u64,
        config: Box<AppConfig>,
        asr_only: bool,
        reply: StartReply,
    },
    Stop {
        session_id: String,
        reply: StopReply,
    },
    Cancel {
        session_id: String,
        reply: mpsc::Sender<()>,
    },
    CancelOwner {
        owner: String,
        epoch: u64,
    },
    Completed {
        session_id: String,
        result: Result<InputTestResult, String>,
        reply: StopReply,
    },
    Shutdown,
}

pub(super) struct TestRecordingController {
    sender: mpsc::Sender<TestCommand>,
    owner_epochs: Arc<OwnerEpochs>,
}

impl TestRecordingController {
    pub(super) fn new(gate: Arc<RecordingGate>, app: AppHandle) -> std::io::Result<Self> {
        let notify = Arc::new(move |event| {
            let _ = app.emit(CAPTURE_EVENT, event);
        });
        let owner_epochs = Arc::new(OwnerEpochs::default());
        let (sender, _worker) = spawn_worker(
            gate,
            owner_epochs.clone(),
            || {
                CpalRecordingSession::start()
                    .map(|session| Box::new(session) as Box<dyn TestRecorder>)
                    .map_err(|error| format!("麦克风录音启动失败：{error}"))
            },
            CaptureLimits::default(),
            notify,
        )?;
        Ok(Self {
            sender,
            owner_epochs,
        })
    }

    pub(super) fn cancel_owner(&self, owner: &str) {
        let epoch = self.owner_epochs.cancel(owner);
        let _ = self.sender.send(TestCommand::CancelOwner {
            owner: owner.to_string(),
            epoch,
        });
    }
}

impl Drop for TestRecordingController {
    fn drop(&mut self) {
        let _ = self.sender.send(TestCommand::Shutdown);
    }
}

fn process_input(input: ProcessingInput) -> Result<InputTestResult, String> {
    if input.cancelled.load(Ordering::Acquire) {
        return Err("测试已取消。".to_string());
    }
    let mut result =
        service_test::process_recorded_input(&input.config, input.audio, input.asr_only)?;
    if let Some(warning) = input.warning {
        result.warnings.push(warning);
    }
    Ok(result)
}

fn spawn_worker<F>(
    gate: Arc<RecordingGate>,
    owner_epochs: Arc<OwnerEpochs>,
    factory: F,
    limits: CaptureLimits,
    notify: Arc<dyn Fn(CaptureEvent) + Send + Sync>,
) -> std::io::Result<(mpsc::Sender<TestCommand>, thread::JoinHandle<()>)>
where
    F: FnMut() -> Result<Box<dyn TestRecorder>, String> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    let worker_sender = sender.clone();
    let worker = thread::Builder::new()
        .name("orally-test-recording".to_string())
        .spawn(move || {
            // Construct the recorder and all stream-owning state on this thread.
            let mut worker = RecordingWorker {
                gate,
                owner_epochs,
                factory: Box::new(factory),
                limits,
                active: None,
            };
            loop {
                match receiver.recv_timeout(WORKER_POLL) {
                    Ok(TestCommand::Start {
                        owner,
                        owner_epoch,
                        config,
                        asr_only,
                        reply,
                    }) => {
                        let result = worker.start(owner, owner_epoch, *config, asr_only);
                        let id = result.as_ref().ok().cloned();
                        if reply.send(result).is_err() {
                            if let Some(id) = id {
                                worker.cancel(&id);
                            }
                        }
                    }
                    Ok(TestCommand::Stop { session_id, reply }) => {
                        match worker.begin_processing(&session_id) {
                            Ok(input) => {
                                let sender = worker_sender.clone();
                                thread::spawn(move || {
                                    let result = std::panic::catch_unwind(
                                        std::panic::AssertUnwindSafe(|| process_input(input)),
                                    )
                                    .unwrap_or_else(|_| {
                                        Err("测试处理失败，请重新录音。".to_string())
                                    });
                                    let _ = sender.send(TestCommand::Completed {
                                        session_id,
                                        result,
                                        reply,
                                    });
                                });
                            }
                            Err(error) => {
                                let _ = reply.send(Err(error));
                            }
                        }
                    }
                    Ok(TestCommand::Cancel { session_id, reply }) => {
                        worker.cancel(&session_id);
                        let _ = reply.send(());
                    }
                    Ok(TestCommand::CancelOwner { owner, epoch }) => {
                        if let Some(event) = worker.cancel_owner(&owner, epoch) {
                            notify(event);
                        }
                    }
                    Ok(TestCommand::Completed {
                        session_id,
                        result,
                        reply,
                    }) => {
                        let result = if worker.complete(&session_id) {
                            result
                        } else {
                            Err("测试已取消，已忽略旧录音结果。".to_string())
                        };
                        let _ = reply.send(result);
                    }
                    Ok(TestCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        if let Some(id) = worker.active.as_ref().map(|session| session.id.clone()) {
                            worker.cancel(&id);
                        }
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if let Some(event) = worker.check_capture_limits(Instant::now()) {
                    notify(event);
                }
            }
        })?;
    Ok((sender, worker))
}

fn unavailable() -> String {
    "录音测试服务不可用，请重启 Orally 后重试。".to_string()
}

#[tauri::command]
pub(super) async fn start_test_recording(
    window: WebviewWindow,
    state: State<'_, TestRecordingController>,
    config: AppConfig,
    asr_only: bool,
) -> Result<String, String> {
    let sender = state.sender.clone();
    let owner = window.label().to_string();
    let owner_epoch = state.owner_epochs.current(&owner);
    let (reply, response) = mpsc::channel();
    sender
        .send(TestCommand::Start {
            owner,
            owner_epoch,
            config: Box::new(config),
            asr_only,
            reply,
        })
        .map_err(|_| unavailable())?;
    tauri::async_runtime::spawn_blocking(move || response.recv().map_err(|_| unavailable())?)
        .await
        .map_err(|_| unavailable())?
}

#[tauri::command]
pub(super) async fn stop_test_recording(
    state: State<'_, TestRecordingController>,
    session_id: String,
) -> Result<InputTestResult, String> {
    let sender = state.sender.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (reply, response) = mpsc::channel();
        sender
            .send(TestCommand::Stop { session_id, reply })
            .map_err(|_| unavailable())?;
        response.recv().map_err(|_| unavailable())?
    })
    .await
    .map_err(|_| unavailable())?
}

#[tauri::command]
pub(super) async fn cancel_test_recording(
    state: State<'_, TestRecordingController>,
    session_id: String,
) -> Result<(), String> {
    let sender = state.sender.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (reply, response) = mpsc::channel();
        sender
            .send(TestCommand::Cancel { session_id, reply })
            .map_err(|_| unavailable())?;
        response.recv().map_err(|_| unavailable())
    })
    .await
    .map_err(|_| unavailable())?
}

pub(super) fn cancel_window(app: &AppHandle, label: &str) {
    if let Some(controller) = app.try_state::<TestRecordingController>() {
        controller.cancel_owner(label);
    }
}

#[cfg(test)]
mod tests;
