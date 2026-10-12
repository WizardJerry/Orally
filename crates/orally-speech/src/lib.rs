//! Shared audio-to-Final-Text processing for Orally application shells.
//!
//! This crate owns ASR adapter selection and transcript refinement. Callers
//! retain recording, privacy policy, insertion, history, and user-interface
//! responsibilities.

use orally_asr::{
    AutoAsrProvider, ChatAudioAsrConfig, ChatAudioAsrProvider, OpenAiCompatibleAsrConfig,
    OpenAiCompatibleAsrProvider,
};
use orally_core::{
    AppContext, AsrProvider, AudioInput, BuiltInTextProcessor, DictionaryTerm, OrallyError,
    PostprocessPrompt, ProcessInput, ProcessedText, TextProcessor, Transcript,
};
use orally_llm::{OpenAiChatPostprocessor, OpenAiChatPostprocessorConfig};
use std::env;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;
use std::time::Duration;

#[derive(Clone, PartialEq, Eq)]
/// A credential supplied directly or read from a named environment variable.
pub struct CredentialSource {
    direct: Option<String>,
    environment_variable: String,
}

impl CredentialSource {
    /// Creates a source where a non-blank direct value takes precedence.
    pub fn new(direct: Option<String>, environment_variable: impl Into<String>) -> Self {
        Self {
            direct,
            environment_variable: environment_variable.into(),
        }
    }

    fn environment_variable(&self) -> &str {
        &self.environment_variable
    }

    fn resolve(&self) -> Option<String> {
        self.resolve_with(|name| env::var(name).ok())
    }

    fn resolve_with(
        &self,
        read_environment: impl FnOnce(&str) -> Option<String>,
    ) -> Option<String> {
        self.direct
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| read_environment(&self.environment_variable))
    }
}

impl Debug for CredentialSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialSource")
            .field("direct", &self.direct.as_ref().map(|_| "<redacted>"))
            .field("environment_variable", &self.environment_variable)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The request shape used by an OpenAI-compatible ASR provider.
pub enum AsrProtocol {
    /// Negotiates compatible formats from HTTP responses, without host or model inference.
    Auto,
    OpenAiTranscriptions,
    ChatAudio,
}

impl FromStr for AsrProtocol {
    type Err = ParseAsrProtocolError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "openai-transcriptions" | "multipart" => Ok(Self::OpenAiTranscriptions),
            "chat-audio" | "chat-completions" => Ok(Self::ChatAudio),
            other => Err(ParseAsrProtocolError {
                value: other.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Returned when an ASR protocol name is not recognized.
pub struct ParseAsrProtocolError {
    value: String,
}

impl Display for ParseAsrProtocolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown ASR protocol: {}; expected auto, openai-transcriptions, or chat-audio",
            self.value
        )
    }
}

impl Error for ParseAsrProtocolError {}

#[derive(Debug, Clone, PartialEq, Eq)]
/// An error encountered while constructing the recognition side of a processor.
pub enum SpeechBuildError {
    MissingRecognitionCredential { environment_variable: String },
    RecognitionAdapter(OrallyError),
}

impl Display for SpeechBuildError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRecognitionCredential {
                environment_variable,
            } => write!(
                f,
                "missing API key: set asr.api_key or env var {environment_variable}"
            ),
            Self::RecognitionAdapter(error) => Display::fmt(error, f),
        }
    }
}

impl Error for SpeechBuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MissingRecognitionCredential { .. } => None,
            Self::RecognitionAdapter(error) => Some(error),
        }
    }
}

impl From<SpeechBuildError> for OrallyError {
    fn from(error: SpeechBuildError) -> Self {
        match error {
            SpeechBuildError::MissingRecognitionCredential {
                environment_variable,
            } => Self::InvalidInput(format!(
                "missing API key: set asr.api_key or env var {environment_variable}"
            )),
            SpeechBuildError::RecognitionAdapter(error) => error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Runtime settings for remote speech recognition.
pub struct RecognitionPlan {
    pub base_url: String,
    pub model: String,
    pub credential: CredentialSource,
    pub protocol: AsrProtocol,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Runtime settings for optional AI Post-processing.
pub struct AiPostprocessPlan {
    pub base_url: String,
    pub model: String,
    pub credential: CredentialSource,
    pub system_prompt: String,
    pub user_template: String,
    pub fallback_to_local_basic_cleanup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// The refinement behavior to apply after recognition succeeds.
pub enum RefinementPlan {
    Raw,
    LocalBasicCleanup,
    AiPostprocessing(AiPostprocessPlan),
    /// Runs models in order, passing each model's text to the next model.
    AiPipeline(Vec<AiPostprocessPlan>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// The complete runtime input needed to construct a speech processor.
pub struct SpeechPlan {
    pub recognition: RecognitionPlan,
    pub refinement: RefinementPlan,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq)]
/// Raw and final text produced by one audio processing operation.
pub struct SpeechOutcome {
    pub raw_transcript: Transcript,
    pub final_text: String,
    pub changes: Vec<String>,
}

/// A configured, reusable audio-to-Final-Text processor.
pub struct SpeechProcessor {
    asr: Box<dyn AsrProvider>,
    refinement: RuntimeRefinement,
    locale: String,
    request_timeout: Duration,
}

enum RuntimeRefinement {
    Raw,
    LocalBasicCleanup,
    Ai {
        processor: AiProcessor,
        fallback_to_local_basic_cleanup: bool,
    },
    AiPipeline(Vec<RuntimeAiStep>),
}

struct RuntimeAiStep {
    processor: AiProcessor,
    fallback_to_local_basic_cleanup: bool,
}

impl From<AiPostprocessPlan> for RuntimeAiStep {
    fn from(plan: AiPostprocessPlan) -> Self {
        Self {
            fallback_to_local_basic_cleanup: plan.fallback_to_local_basic_cleanup,
            processor: AiProcessor::Plan(plan),
        }
    }
}

enum AiProcessor {
    Plan(AiPostprocessPlan),
    #[cfg(test)]
    Adapter(Box<dyn TextProcessor>),
}

impl From<RefinementPlan> for RuntimeRefinement {
    fn from(plan: RefinementPlan) -> Self {
        match plan {
            RefinementPlan::Raw => Self::Raw,
            RefinementPlan::LocalBasicCleanup => Self::LocalBasicCleanup,
            RefinementPlan::AiPostprocessing(plan) => Self::Ai {
                fallback_to_local_basic_cleanup: plan.fallback_to_local_basic_cleanup,
                processor: AiProcessor::Plan(plan),
            },
            RefinementPlan::AiPipeline(plans) => {
                Self::AiPipeline(plans.into_iter().map(RuntimeAiStep::from).collect())
            }
        }
    }
}

impl SpeechProcessor {
    /// Builds the recognition adapter and stores refinement settings.
    pub fn new(plan: SpeechPlan) -> Result<Self, SpeechBuildError> {
        Self::new_with_timeout(plan, Duration::from_secs(120))
    }

    /// Builds a processor with a time limit for each external request.
    pub fn new_with_timeout(
        plan: SpeechPlan,
        request_timeout: Duration,
    ) -> Result<Self, SpeechBuildError> {
        let asr = build_asr_provider(plan.recognition, request_timeout)?;
        Ok(Self {
            asr,
            refinement: plan.refinement.into(),
            locale: plan.locale,
            request_timeout,
        })
    }

    /// Recognizes one audio input and applies the configured refinement path.
    pub fn process(&self, audio: AudioInput) -> Result<SpeechOutcome, OrallyError> {
        let transcript = self.asr.transcribe(audio)?;
        apply_refinement(
            &self.refinement,
            transcript,
            &self.locale,
            self.request_timeout,
        )
    }

    #[cfg(test)]
    fn with_adapters(
        asr: Box<dyn AsrProvider>,
        refinement: TestRefinement,
        locale: impl Into<String>,
    ) -> Self {
        let refinement = match refinement {
            TestRefinement::Raw => RuntimeRefinement::Raw,
            TestRefinement::LocalBasicCleanup => RuntimeRefinement::LocalBasicCleanup,
            TestRefinement::Ai {
                processor,
                fallback_to_local_basic_cleanup,
            } => RuntimeRefinement::Ai {
                processor: AiProcessor::Adapter(processor),
                fallback_to_local_basic_cleanup,
            },
            TestRefinement::AiPlan(plan) => RuntimeRefinement::Ai {
                fallback_to_local_basic_cleanup: plan.fallback_to_local_basic_cleanup,
                processor: AiProcessor::Plan(plan),
            },
            TestRefinement::AiPipeline(steps) => RuntimeRefinement::AiPipeline(
                steps
                    .into_iter()
                    .map(
                        |(processor, fallback_to_local_basic_cleanup)| RuntimeAiStep {
                            processor: AiProcessor::Adapter(processor),
                            fallback_to_local_basic_cleanup,
                        },
                    )
                    .collect(),
            ),
            TestRefinement::AiPipelinePlan(plans) => {
                RuntimeRefinement::AiPipeline(plans.into_iter().map(RuntimeAiStep::from).collect())
            }
        };

        Self {
            asr,
            refinement,
            locale: locale.into(),
            request_timeout: Duration::from_secs(120),
        }
    }
}

/// Applies the normal refinement path to text supplied without speech recognition.
pub fn refine_transcript(
    plan: RefinementPlan,
    transcript: Transcript,
    locale: &str,
    request_timeout: Duration,
) -> Result<SpeechOutcome, OrallyError> {
    apply_refinement(&plan.into(), transcript, locale, request_timeout)
}

fn apply_refinement(
    refinement: &RuntimeRefinement,
    transcript: Transcript,
    locale: &str,
    request_timeout: Duration,
) -> Result<SpeechOutcome, OrallyError> {
    let raw_transcript = transcript.clone();
    let processed = match refinement {
        RuntimeRefinement::Raw => ProcessedText {
            text: transcript.text,
            changes: Vec::new(),
        },
        RuntimeRefinement::LocalBasicCleanup => {
            run_local_cleanup(process_input(transcript, locale))?
        }
        RuntimeRefinement::Ai {
            processor,
            fallback_to_local_basic_cleanup,
        } => run_ai_refinement(
            processor,
            process_input(transcript, locale),
            *fallback_to_local_basic_cleanup,
            request_timeout,
        )?,
        RuntimeRefinement::AiPipeline(steps) => {
            run_ai_pipeline(steps, process_input(transcript, locale), request_timeout)?
        }
    };
    Ok(SpeechOutcome {
        raw_transcript,
        final_text: processed.text,
        changes: processed.changes,
    })
}

fn build_asr_provider(
    plan: RecognitionPlan,
    request_timeout: Duration,
) -> Result<Box<dyn AsrProvider>, SpeechBuildError> {
    let api_key = plan.credential.resolve().ok_or_else(|| {
        SpeechBuildError::MissingRecognitionCredential {
            environment_variable: plan.credential.environment_variable().to_string(),
        }
    })?;

    match plan.protocol {
        AsrProtocol::Auto | AsrProtocol::OpenAiTranscriptions => {
            let mut config = OpenAiCompatibleAsrConfig::new(plan.base_url, api_key, plan.model);
            config.language = plan.language;
            config.timeout = request_timeout;
            if plan.protocol == AsrProtocol::Auto {
                AutoAsrProvider::new(config)
                    .map(|provider| Box::new(provider) as Box<dyn AsrProvider>)
            } else {
                OpenAiCompatibleAsrProvider::new(config)
                    .map(|provider| Box::new(provider) as Box<dyn AsrProvider>)
            }
            .map_err(SpeechBuildError::RecognitionAdapter)
        }
        AsrProtocol::ChatAudio => {
            let mut config = ChatAudioAsrConfig::new(plan.base_url, api_key, plan.model);
            config.language = plan.language;
            config.timeout = request_timeout;
            ChatAudioAsrProvider::new(config)
                .map(|provider| Box::new(provider) as Box<dyn AsrProvider>)
                .map_err(SpeechBuildError::RecognitionAdapter)
        }
    }
}

fn run_ai_refinement(
    source: &AiProcessor,
    input: ProcessInput,
    fallback_to_local_basic_cleanup: bool,
    request_timeout: Duration,
) -> Result<ProcessedText, OrallyError> {
    match source {
        AiProcessor::Plan(plan) => {
            let processor = build_ai_processor(plan, request_timeout)?;
            apply_ai_processor(&processor, input, fallback_to_local_basic_cleanup)
        }
        #[cfg(test)]
        AiProcessor::Adapter(processor) => {
            apply_ai_processor(processor.as_ref(), input, fallback_to_local_basic_cleanup)
        }
    }
}

fn run_ai_pipeline(
    steps: &[RuntimeAiStep],
    mut input: ProcessInput,
    request_timeout: Duration,
) -> Result<ProcessedText, OrallyError> {
    let mut changes = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let processed = run_ai_refinement(
            &step.processor,
            input.clone(),
            step.fallback_to_local_basic_cleanup,
            request_timeout,
        )
        .map_err(|error| annotate_model_failure(error, index, &step.processor))?;
        input.transcript.text = processed.text;
        // Recognition timestamps describe the original text, not model output.
        input.transcript.segments.clear();
        changes.extend(processed.changes.into_iter().map(|change| {
            if change.starts_with("AI postprocessor failed") {
                format!("post-processing model node {}: {change}", index + 1)
            } else {
                change
            }
        }));
    }

    Ok(ProcessedText {
        text: input.transcript.text,
        changes,
    })
}

fn annotate_model_failure(
    error: OrallyError,
    index: usize,
    processor: &AiProcessor,
) -> OrallyError {
    let model = match processor {
        AiProcessor::Plan(plan) => format!(" ({})", plan.model),
        #[cfg(test)]
        AiProcessor::Adapter(_) => String::new(),
    };
    let prefix = format!("post-processing model node {}{model} failed: ", index + 1);
    match error {
        OrallyError::InvalidInput(message) => OrallyError::InvalidInput(prefix + &message),
        OrallyError::Audio(message) => OrallyError::Audio(prefix + &message),
        OrallyError::Asr(message) => OrallyError::Asr(prefix + &message),
        OrallyError::Processing(message) => OrallyError::Processing(prefix + &message),
        OrallyError::Insertion(message) => OrallyError::Insertion(prefix + &message),
    }
}

fn build_ai_processor(
    plan: &AiPostprocessPlan,
    request_timeout: Duration,
) -> Result<OpenAiChatPostprocessor, OrallyError> {
    let api_key = plan.credential.resolve().ok_or_else(|| {
        OrallyError::InvalidInput(format!(
            "missing postprocess API key: set postprocess.api_key or env var {}",
            plan.credential.environment_variable()
        ))
    })?;
    let mut config =
        OpenAiChatPostprocessorConfig::new(plan.base_url.clone(), api_key, plan.model.clone());
    config.system_prompt = plan.system_prompt.clone();
    config.user_template = plan.user_template.clone();
    config.timeout = request_timeout;
    OpenAiChatPostprocessor::new(config)
}

fn apply_ai_processor(
    processor: &dyn TextProcessor,
    input: ProcessInput,
    fallback_to_local_basic_cleanup: bool,
) -> Result<ProcessedText, OrallyError> {
    match processor.process(input.clone()) {
        Ok(processed) => Ok(processed),
        Err(error) if fallback_to_local_basic_cleanup => {
            let mut processed = run_local_cleanup(input)?;
            processed.changes.push(format!(
                "AI postprocessor failed; used built-in cleanup: {error}"
            ));
            Ok(processed)
        }
        Err(error) => Err(error),
    }
}

fn run_local_cleanup(input: ProcessInput) -> Result<ProcessedText, OrallyError> {
    BuiltInTextProcessor.process(input)
}

fn process_input(transcript: Transcript, locale: &str) -> ProcessInput {
    ProcessInput {
        transcript,
        context: AppContext {
            locale: locale.to_string(),
            ..AppContext::default()
        },
        prompt: PostprocessPrompt::default(),
        dictionary_terms: default_dictionary(),
    }
}

fn default_dictionary() -> Vec<DictionaryTerm> {
    vec![
        DictionaryTerm {
            spoken: "visual studio code".to_string(),
            written: "Visual Studio Code".to_string(),
        },
        DictionaryTerm {
            spoken: "orally".to_string(),
            written: "Orally".to_string(),
        },
    ]
}

#[cfg(test)]
enum TestRefinement {
    Raw,
    LocalBasicCleanup,
    Ai {
        processor: Box<dyn TextProcessor>,
        fallback_to_local_basic_cleanup: bool,
    },
    AiPlan(AiPostprocessPlan),
    AiPipeline(Vec<(Box<dyn TextProcessor>, bool)>),
    AiPipelinePlan(Vec<AiPostprocessPlan>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use orally_core::TranscriptSegment;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    struct FakeAsr {
        result: Result<Transcript, OrallyError>,
        calls: Arc<AtomicUsize>,
    }

    impl AsrProvider for FakeAsr {
        fn transcribe(&self, _audio: AudioInput) -> Result<Transcript, OrallyError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result.clone()
        }
    }

    struct FakeProcessor {
        result: Result<ProcessedText, OrallyError>,
        calls: Arc<AtomicUsize>,
    }

    impl TextProcessor for FakeProcessor {
        fn process(&self, _input: ProcessInput) -> Result<ProcessedText, OrallyError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result.clone()
        }
    }

    struct RecordingProcessor {
        result: Result<ProcessedText, OrallyError>,
        inputs: Arc<Mutex<Vec<ProcessInput>>>,
    }

    impl TextProcessor for RecordingProcessor {
        fn process(&self, input: ProcessInput) -> Result<ProcessedText, OrallyError> {
            self.inputs.lock().unwrap().push(input);
            self.result.clone()
        }
    }

    fn recording_step(
        result: Result<ProcessedText, OrallyError>,
        inputs: &Arc<Mutex<Vec<ProcessInput>>>,
        fallback: bool,
    ) -> (Box<dyn TextProcessor>, bool) {
        (
            Box::new(RecordingProcessor {
                result,
                inputs: inputs.clone(),
            }),
            fallback,
        )
    }

    fn transcript(text: &str) -> Transcript {
        Transcript {
            text: text.to_string(),
            language: Some("zh".to_string()),
            segments: Vec::new(),
        }
    }

    fn fake_asr(
        result: Result<Transcript, OrallyError>,
        calls: Arc<AtomicUsize>,
    ) -> Box<dyn AsrProvider> {
        Box::new(FakeAsr { result, calls })
    }

    #[test]
    fn auto_recognition_uses_standard_transcription_on_a_generic_host() {
        let models = ["renamed-asr", "provider/transcriber-v2", "audio-model"];
        let (base_url, server) = serve_recognition_responses(
            models
                .iter()
                .map(|_| "{\"text\":\"recognized speech\",\"language\":\"zh\"}".to_string())
                .collect(),
        );
        for model in models {
            let processor = SpeechProcessor::new_with_timeout(
                SpeechPlan {
                    recognition: RecognitionPlan {
                        base_url: base_url.clone(),
                        model: model.to_string(),
                        credential: CredentialSource::new(Some("fixture-key".to_string()), ""),
                        protocol: AsrProtocol::Auto,
                        language: Some("zh".to_string()),
                    },
                    refinement: RefinementPlan::Raw,
                    locale: "zh-CN".to_string(),
                },
                Duration::from_secs(5),
            )
            .unwrap();

            let result = processor.process(AudioInput {
                bytes: vec![0, 0, 1, 0],
                sample_rate_hz: 16_000,
                channels: 1,
                format: orally_core::AudioFormat::Pcm16,
            });
            let outcome = result.unwrap();
            assert_eq!(outcome.raw_transcript.text, "recognized speech");
            assert_eq!(outcome.final_text, "recognized speech");
        }
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), models.len());
        for (request, model) in requests.iter().zip(models) {
            assert!(
                request.starts_with("POST /v1/audio/transcriptions "),
                "Auto must use standard multipart transcription; sent {}",
                request.lines().next().unwrap()
            );
            assert!(request.contains("multipart/form-data"));
            assert!(request.contains("authorization: Bearer fixture-key"));
            assert!(request.contains(model));
            assert!(!request.contains("name=\"prompt\""));
            assert!(request.contains("name=\"language\"\r\n\r\nzh"));
            assert!(request.contains("RIFF"));
            assert!(request.contains("WAVE"));
        }
    }

    #[test]
    fn auto_negotiates_chat_data_url_after_missing_transcription_endpoint() {
        let (base_url, server) = serve_negotiation_responses(vec![
            (
                "404 Not Found",
                "{\"error\":\"route unavailable\"}".to_string(),
            ),
            (
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"recognized speech\"}}]}".to_string(),
            ),
        ]);
        let processor = SpeechProcessor::new_with_timeout(
            SpeechPlan {
                recognition: RecognitionPlan {
                    base_url,
                    model: "audio-transcriber".to_string(),
                    credential: CredentialSource::new(Some("fixture-key".to_string()), ""),
                    protocol: AsrProtocol::Auto,
                    language: Some("zh".to_string()),
                },
                refinement: RefinementPlan::Raw,
                locale: "zh-CN".to_string(),
            },
            Duration::from_secs(5),
        )
        .unwrap();
        let result = processor.process(AudioInput {
            bytes: vec![0, 0, 1, 0],
            sample_rate_hz: 16_000,
            channels: 1,
            format: orally_core::AudioFormat::Pcm16,
        });
        let requests = server.join().unwrap();
        assert!(
            result.is_ok(),
            "Auto should negotiate a compatible chat route after HTTP 404: {result:?}"
        );
        assert_eq!(requests.len(), 2);
        assert!(requests[0].starts_with("POST /v1/audio/transcriptions "));
        assert!(requests[1].starts_with("POST /v1/chat/completions "));
        assert!(requests[1].contains("data:audio/wav;base64,"));
        assert!(!requests[1].contains("\"format\""));
        assert!(requests[1].contains("\"role\":\"system\""));
        assert!(requests[1].contains("Language hint: zh"));
        assert!(!requests[1].contains("Please transcribe"));
        assert_eq!(result.unwrap().final_text, "recognized speech");
    }

    #[test]
    fn recognition_and_refinement_share_a_full_service_url_and_credential() {
        let (base_url, server) = serve_recognition_responses(vec![
            "{\"text\":\"raw speech\"}".to_string(),
            "{\"choices\":[{\"message\":{\"content\":\"final speech\"}}]}".to_string(),
        ]);
        let shared_url = format!(" {base_url}/chat/completions/?route=shared#ignored ");
        let credential = CredentialSource::new(Some("shared-fixture-key".to_string()), "");
        let processor = SpeechProcessor::new_with_timeout(
            SpeechPlan {
                recognition: RecognitionPlan {
                    base_url: shared_url.clone(),
                    model: "shared-asr-model".to_string(),
                    credential: credential.clone(),
                    protocol: AsrProtocol::Auto,
                    language: None,
                },
                refinement: RefinementPlan::AiPostprocessing(AiPostprocessPlan {
                    base_url: shared_url,
                    model: "shared-llm-model".to_string(),
                    credential,
                    system_prompt: "Preserve the meaning".to_string(),
                    user_template: "{{transcript}}".to_string(),
                    fallback_to_local_basic_cleanup: false,
                }),
                locale: "en-US".to_string(),
            },
            Duration::from_secs(5),
        )
        .unwrap();
        let result = processor.process(AudioInput {
            bytes: vec![0, 0, 1, 0],
            sample_rate_hz: 16_000,
            channels: 1,
            format: orally_core::AudioFormat::Pcm16,
        });
        let requests = server.join().unwrap();
        let outcome = result.unwrap();
        assert_eq!(outcome.raw_transcript.text, "raw speech");
        assert_eq!(outcome.final_text, "final speech");
        assert!(requests[0].starts_with("POST /v1/audio/transcriptions?route=shared "));
        assert!(requests[0].contains("multipart/form-data"));
        assert!(requests[0].contains("shared-asr-model"));
        assert!(requests[1].starts_with("POST /v1/chat/completions?route=shared "));
        assert!(requests[1].contains("\"model\":\"shared-llm-model\""));
        assert!(requests[1].contains("\"content\":\"raw speech\""));
        for request in requests {
            assert!(request.contains("authorization: Bearer shared-fixture-key"));
        }
    }

    #[test]
    fn raw_preserves_transcript_without_changes() {
        let processor = SpeechProcessor::with_adapters(
            fake_asr(Ok(transcript("原始 文本")), Arc::new(AtomicUsize::new(0))),
            TestRefinement::Raw,
            "zh-CN",
        );

        let outcome = processor
            .process(AudioInput::demo_text("ignored"))
            .expect("raw processing should succeed");

        assert_eq!(outcome.raw_transcript.text, "原始 文本");
        assert_eq!(outcome.final_text, "原始 文本");
        assert!(outcome.changes.is_empty());
    }

    #[test]
    fn local_cleanup_applies_current_dictionary() {
        let processor = SpeechProcessor::with_adapters(
            fake_asr(
                Ok(transcript("嗯  使用 visual studio code")),
                Arc::new(AtomicUsize::new(0)),
            ),
            TestRefinement::LocalBasicCleanup,
            "zh-CN",
        );

        let outcome = processor
            .process(AudioInput::demo_text("ignored"))
            .expect("local cleanup should succeed");

        assert_eq!(outcome.final_text, "使用 Visual Studio Code。");
    }

    #[test]
    fn ai_success_returns_ai_outcome() {
        let ai_calls = Arc::new(AtomicUsize::new(0));
        let processor = SpeechProcessor::with_adapters(
            fake_asr(Ok(transcript("raw")), Arc::new(AtomicUsize::new(0))),
            TestRefinement::Ai {
                processor: Box::new(FakeProcessor {
                    result: Ok(ProcessedText {
                        text: "polished".to_string(),
                        changes: vec!["ai".to_string()],
                    }),
                    calls: ai_calls.clone(),
                }),
                fallback_to_local_basic_cleanup: true,
            },
            "zh-CN",
        );

        let outcome = processor
            .process(AudioInput::demo_text("ignored"))
            .expect("AI processing should succeed");

        assert_eq!(outcome.final_text, "polished");
        assert_eq!(ai_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn ai_failure_falls_back_to_local_cleanup() {
        let processor = SpeechProcessor::with_adapters(
            fake_asr(
                Ok(transcript("嗯 hello world")),
                Arc::new(AtomicUsize::new(0)),
            ),
            TestRefinement::Ai {
                processor: Box::new(FakeProcessor {
                    result: Err(OrallyError::Processing("offline".to_string())),
                    calls: Arc::new(AtomicUsize::new(0)),
                }),
                fallback_to_local_basic_cleanup: true,
            },
            "en-US",
        );

        let outcome = processor
            .process(AudioInput::demo_text("ignored"))
            .expect("fallback should succeed");

        assert_eq!(outcome.final_text, "hello world.");
        assert!(outcome
            .changes
            .iter()
            .any(|change| change.contains("offline")));
    }

    #[test]
    fn ai_failure_without_fallback_is_returned() {
        let processor = SpeechProcessor::with_adapters(
            fake_asr(Ok(transcript("raw")), Arc::new(AtomicUsize::new(0))),
            TestRefinement::Ai {
                processor: Box::new(FakeProcessor {
                    result: Err(OrallyError::Processing("offline".to_string())),
                    calls: Arc::new(AtomicUsize::new(0)),
                }),
                fallback_to_local_basic_cleanup: false,
            },
            "zh-CN",
        );

        let error = processor
            .process(AudioInput::demo_text("ignored"))
            .expect_err("AI failure should be returned");

        assert_eq!(error, OrallyError::Processing("offline".to_string()));
    }

    #[test]
    fn ai_pipeline_passes_text_in_order_and_preserves_raw_transcript() {
        let inputs = Arc::new(Mutex::new(Vec::new()));
        let mut raw = transcript("raw speech");
        raw.segments.push(TranscriptSegment {
            start_ms: 0,
            end_ms: 1000,
            text: raw.text.clone(),
            confidence: Some(0.9),
        });
        let processor = SpeechProcessor::with_adapters(
            fake_asr(Ok(raw.clone()), Arc::new(AtomicUsize::new(0))),
            TestRefinement::AiPipeline(vec![
                recording_step(
                    Ok(ProcessedText {
                        text: "first output".to_string(),
                        changes: vec!["first model".to_string()],
                    }),
                    &inputs,
                    false,
                ),
                recording_step(
                    Ok(ProcessedText {
                        text: "final output".to_string(),
                        changes: vec!["second model".to_string()],
                    }),
                    &inputs,
                    false,
                ),
            ]),
            "zh-CN",
        );

        let outcome = processor.process(AudioInput::demo_text("ignored")).unwrap();
        let inputs = inputs.lock().unwrap();

        assert_eq!(outcome.raw_transcript, raw);
        assert_eq!(outcome.final_text, "final output");
        assert_eq!(outcome.changes, ["first model", "second model"]);
        assert_eq!(inputs[0].transcript, raw);
        assert_eq!(inputs[1].transcript.text, "first output");
        assert_eq!(inputs[1].transcript.language, raw.language);
        assert!(inputs[1].transcript.segments.is_empty());
        assert_eq!(inputs[1].context.locale, "zh-CN");
    }

    #[test]
    fn empty_ai_pipeline_returns_raw_text() {
        let processor = SpeechProcessor::with_adapters(
            fake_asr(Ok(transcript("嗯 raw")), Arc::new(AtomicUsize::new(0))),
            TestRefinement::AiPipeline(Vec::new()),
            "en-US",
        );

        let outcome = processor.process(AudioInput::demo_text("ignored")).unwrap();

        assert_eq!(outcome.final_text, "嗯 raw");
        assert!(outcome.changes.is_empty());
    }

    #[test]
    fn ai_pipeline_continues_after_local_fallback() {
        let inputs = Arc::new(Mutex::new(Vec::new()));
        let processor = SpeechProcessor::with_adapters(
            fake_asr(
                Ok(transcript("嗯 hello world")),
                Arc::new(AtomicUsize::new(0)),
            ),
            TestRefinement::AiPipeline(vec![
                recording_step(
                    Err(OrallyError::Processing("offline".to_string())),
                    &inputs,
                    true,
                ),
                recording_step(
                    Ok(ProcessedText {
                        text: "final output".to_string(),
                        changes: vec!["second model".to_string()],
                    }),
                    &inputs,
                    false,
                ),
            ]),
            "en-US",
        );

        let outcome = processor.process(AudioInput::demo_text("ignored")).unwrap();

        assert_eq!(inputs.lock().unwrap()[1].transcript.text, "hello world.");
        assert_eq!(outcome.final_text, "final output");
        assert!(outcome
            .changes
            .iter()
            .any(|change| change.contains("offline")));
        assert_eq!(outcome.changes.last().unwrap(), "second model");
    }

    #[test]
    fn ai_pipeline_stops_after_failure_without_fallback() {
        let inputs = Arc::new(Mutex::new(Vec::new()));
        let processor = SpeechProcessor::with_adapters(
            fake_asr(Ok(transcript("raw")), Arc::new(AtomicUsize::new(0))),
            TestRefinement::AiPipeline(vec![
                recording_step(
                    Err(OrallyError::Processing("offline".to_string())),
                    &inputs,
                    false,
                ),
                recording_step(
                    Ok(ProcessedText {
                        text: "should not run".to_string(),
                        changes: Vec::new(),
                    }),
                    &inputs,
                    true,
                ),
            ]),
            "zh-CN",
        );

        let error = processor
            .process(AudioInput::demo_text("ignored"))
            .unwrap_err();

        assert_eq!(
            error,
            OrallyError::Processing("post-processing model node 1 failed: offline".to_string())
        );
        assert_eq!(inputs.lock().unwrap().len(), 1);
    }

    #[test]
    fn ai_pipeline_missing_credential_does_not_fallback() {
        let asr_calls = Arc::new(AtomicUsize::new(0));
        let processor = SpeechProcessor::with_adapters(
            fake_asr(Ok(transcript("raw")), asr_calls.clone()),
            TestRefinement::AiPipelinePlan(vec![AiPostprocessPlan {
                base_url: "http://127.0.0.1:1/v1".to_string(),
                model: "model".to_string(),
                credential: CredentialSource::new(None, ""),
                system_prompt: "system".to_string(),
                user_template: "{{transcript}}".to_string(),
                fallback_to_local_basic_cleanup: true,
            }]),
            "zh-CN",
        );

        let error = processor
            .process(AudioInput::demo_text("ignored"))
            .unwrap_err();

        assert_eq!(asr_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            error,
            OrallyError::InvalidInput(
                "post-processing model node 1 (model) failed: missing postprocess API key: set postprocess.api_key or env var ".to_string()
            )
        );
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let count = stream.read(&mut buffer).expect("read local request");
            assert!(
                count > 0,
                "client disconnected before the request was complete"
            );
            request.extend_from_slice(&buffer[..count]);
            if let Some(header_end) = request.windows(4).position(|value| value == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .expect("request includes content length");
                if request.len() >= header_end + 4 + content_length {
                    return String::from_utf8_lossy(&request).to_string();
                }
            }
        }
    }

    fn serve_recognition_responses(
        responses: Vec<String>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for body in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "ASR did not send its request");
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("accept local request: {error}"),
                    }
                };
                requests.push(read_http_request(&mut stream));
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (base_url, server)
    }

    fn serve_negotiation_responses(
        responses: Vec<(&'static str, String)>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let deadline = Instant::now() + Duration::from_secs(1);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if Instant::now() >= deadline {
                                return requests;
                            }
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("accept local request: {error}"),
                    }
                };
                requests.push(read_http_request(&mut stream));
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (base_url, server)
    }

    #[test]
    fn ai_pipeline_sends_each_models_prompt_and_previous_output_over_http() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for text in ["first output", "final output"] {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "model did not send its request");
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("accept local request: {error}"),
                    }
                };
                requests.push(read_http_request(&mut stream));
                let body = format!("{{\"choices\":[{{\"message\":{{\"content\":\"{text}\"}}}}]}}");
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
            requests
        });
        let plans = [
            ("first-model", "base\n\nfirst prompt\n\nsecond prompt"),
            ("second-model", "second instructions"),
        ]
        .into_iter()
        .map(|(model, system_prompt)| AiPostprocessPlan {
            base_url: format!("http://{address}/v1"),
            model: model.to_string(),
            credential: CredentialSource::new(Some("test-key".to_string()), ""),
            system_prompt: system_prompt.to_string(),
            user_template: "{{locale}}: {{transcript}}".to_string(),
            fallback_to_local_basic_cleanup: false,
        })
        .collect();
        let processor = SpeechProcessor::with_adapters(
            fake_asr(Ok(transcript("raw speech")), Arc::new(AtomicUsize::new(0))),
            TestRefinement::AiPipelinePlan(plans),
            "zh-CN",
        );

        let outcome = processor.process(AudioInput::demo_text("ignored"));
        let requests = server.join().unwrap();
        let outcome = outcome.expect("both model requests should succeed");

        assert_eq!(outcome.raw_transcript.text, "raw speech");
        assert_eq!(outcome.final_text, "final output");
        assert_eq!(outcome.changes.len(), 2);
        assert!(requests[0].starts_with("POST /v1/chat/completions "));
        assert!(requests[0].contains("\"model\":\"first-model\""));
        assert!(requests[0].contains("\"content\":\"base\\n\\nfirst prompt\\n\\nsecond prompt\""));
        assert!(requests[0].contains("\"content\":\"zh-CN: raw speech\""));
        assert!(requests[1].contains("\"model\":\"second-model\""));
        assert!(requests[1].contains("\"content\":\"second instructions\""));
        assert!(requests[1].contains("\"content\":\"zh-CN: first output\""));
        assert!(!requests[1].contains("raw speech"));
    }

    #[test]
    fn asr_failure_does_not_run_refinement() {
        let plan = AiPostprocessPlan {
            base_url: "not-a-valid-url".to_string(),
            model: "model".to_string(),
            credential: CredentialSource::new(None, ""),
            system_prompt: "system".to_string(),
            user_template: "{{transcript}}".to_string(),
            fallback_to_local_basic_cleanup: true,
        };
        let processor = SpeechProcessor::with_adapters(
            fake_asr(
                Err(OrallyError::Asr("unavailable".to_string())),
                Arc::new(AtomicUsize::new(0)),
            ),
            TestRefinement::AiPlan(plan),
            "zh-CN",
        );

        let error = processor
            .process(AudioInput::demo_text("ignored"))
            .expect_err("ASR failure should be returned");

        assert_eq!(error, OrallyError::Asr("unavailable".to_string()));
    }

    #[test]
    fn missing_ai_credential_is_checked_after_asr_and_does_not_fallback() {
        let asr_calls = Arc::new(AtomicUsize::new(0));
        let plan = AiPostprocessPlan {
            base_url: "https://example.com/v1".to_string(),
            model: "model".to_string(),
            credential: CredentialSource::new(None, ""),
            system_prompt: "system".to_string(),
            user_template: "{{transcript}}".to_string(),
            fallback_to_local_basic_cleanup: true,
        };
        let processor = SpeechProcessor::with_adapters(
            fake_asr(Ok(transcript("raw")), asr_calls.clone()),
            TestRefinement::AiPlan(plan),
            "zh-CN",
        );

        let error = processor
            .process(AudioInput::demo_text("ignored"))
            .expect_err("missing AI credential should be returned");

        assert_eq!(asr_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            error,
            OrallyError::InvalidInput(
                "missing postprocess API key: set postprocess.api_key or env var ".to_string()
            )
        );
    }

    #[test]
    fn ai_adapter_construction_failure_occurs_after_asr_and_does_not_fallback() {
        let asr_calls = Arc::new(AtomicUsize::new(0));
        let plan = AiPostprocessPlan {
            base_url: "not-a-valid-url".to_string(),
            model: " ".to_string(),
            credential: CredentialSource::new(Some("test-key".to_string()), ""),
            system_prompt: "system".to_string(),
            user_template: "{{transcript}}".to_string(),
            fallback_to_local_basic_cleanup: true,
        };
        let processor = SpeechProcessor::with_adapters(
            fake_asr(Ok(transcript("raw")), asr_calls.clone()),
            TestRefinement::AiPlan(plan),
            "zh-CN",
        );

        let error = processor
            .process(AudioInput::demo_text("ignored"))
            .expect_err("AI adapter construction failure should be returned");

        assert_eq!(asr_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            error,
            OrallyError::InvalidInput("postprocess model is required".to_string())
        );
    }

    #[test]
    fn missing_asr_credential_is_a_typed_build_error() {
        let result = SpeechProcessor::new(SpeechPlan {
            recognition: RecognitionPlan {
                base_url: "https://example.com/v1".to_string(),
                model: "model".to_string(),
                credential: CredentialSource::new(None, ""),
                protocol: AsrProtocol::OpenAiTranscriptions,
                language: None,
            },
            refinement: RefinementPlan::Raw,
            locale: "zh-CN".to_string(),
        });
        let error = match result {
            Ok(_) => panic!("missing ASR credential should fail construction"),
            Err(error) => error,
        };

        assert!(matches!(
            &error,
            SpeechBuildError::MissingRecognitionCredential {
                environment_variable
            } if environment_variable.is_empty()
        ));
        assert_eq!(
            error.to_string(),
            "missing API key: set asr.api_key or env var "
        );
        assert_eq!(
            OrallyError::from(error),
            OrallyError::InvalidInput("missing API key: set asr.api_key or env var ".to_string())
        );
    }

    #[test]
    fn asr_adapter_construction_errors_are_typed_for_both_protocols() {
        for protocol in [AsrProtocol::OpenAiTranscriptions, AsrProtocol::ChatAudio] {
            let result = SpeechProcessor::new(SpeechPlan {
                recognition: RecognitionPlan {
                    base_url: "https://example.com/v1".to_string(),
                    model: " ".to_string(),
                    credential: CredentialSource::new(Some("test-key".to_string()), ""),
                    protocol,
                    language: None,
                },
                refinement: RefinementPlan::Raw,
                locale: "zh-CN".to_string(),
            });
            let error = match result {
                Ok(_) => panic!("invalid ASR config should fail construction"),
                Err(error) => error,
            };

            assert_eq!(
                error,
                SpeechBuildError::RecognitionAdapter(OrallyError::InvalidInput(
                    "ASR model is required".to_string()
                ))
            );
        }
    }

    #[test]
    fn credential_resolution_prefers_direct_and_falls_back_for_blank_direct() {
        let direct = CredentialSource::new(Some("direct-key".to_string()), "TEST_KEY");
        let fallback = CredentialSource::new(Some("  ".to_string()), "TEST_KEY");
        let missing = CredentialSource::new(None, "");

        assert_eq!(
            direct.resolve_with(|_| panic!("direct credential should win")),
            Some("direct-key".to_string())
        );
        assert_eq!(
            fallback.resolve_with(|name| {
                assert_eq!(name, "TEST_KEY");
                Some("environment-key".to_string())
            }),
            Some("environment-key".to_string())
        );
        assert_eq!(missing.resolve_with(|_| None), None);
    }

    #[test]
    fn protocol_aliases_remain_compatible() {
        assert_eq!(
            "multipart"
                .parse::<AsrProtocol>()
                .expect("alias should parse"),
            AsrProtocol::OpenAiTranscriptions
        );
        assert_eq!(
            "chat-completions"
                .parse::<AsrProtocol>()
                .expect("alias should parse"),
            AsrProtocol::ChatAudio
        );
        assert_eq!("auto".parse::<AsrProtocol>().unwrap(), AsrProtocol::Auto);
    }

    #[test]
    fn credential_debug_output_is_redacted() {
        let credential =
            CredentialSource::new(Some("super-secret".to_string()), "ORALLY_TEST_API_KEY");
        let debug = format!("{credential:?}");

        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("ORALLY_TEST_API_KEY"));
    }
}
