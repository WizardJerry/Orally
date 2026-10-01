//! Shared audio-to-Final-Text processing for Orally application shells.
//!
//! This crate owns ASR adapter selection and transcript refinement. Callers
//! retain recording, privacy policy, insertion, history, and user-interface
//! responsibilities.

use orally_asr::{
    ChatAudioAsrConfig, ChatAudioAsrProvider, OpenAiCompatibleAsrConfig,
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
    Auto,
    OpenAiTranscriptions,
    ChatAudio,
}

impl AsrProtocol {
    fn resolve(self, base_url: &str, model: &str) -> Self {
        if self != Self::Auto {
            return self;
        }

        let base_url = base_url.to_ascii_lowercase();
        let model = model.to_ascii_lowercase();
        if base_url.contains("openrouter.ai")
            || base_url.contains("maas.aliyuncs.com")
            || model.contains("qwen3-asr")
        {
            Self::ChatAudio
        } else {
            Self::OpenAiTranscriptions
        }
    }
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
    pub prompt: Option<String>,
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
}

enum RuntimeRefinement {
    Raw,
    LocalBasicCleanup,
    Ai {
        processor: AiProcessor,
        fallback_to_local_basic_cleanup: bool,
    },
}

enum AiProcessor {
    Plan(AiPostprocessPlan),
    #[cfg(test)]
    Adapter(Box<dyn TextProcessor>),
}

impl SpeechProcessor {
    /// Builds the recognition adapter and stores refinement settings.
    pub fn new(plan: SpeechPlan) -> Result<Self, SpeechBuildError> {
        let asr = build_asr_provider(plan.recognition)?;
        let refinement = match plan.refinement {
            RefinementPlan::Raw => RuntimeRefinement::Raw,
            RefinementPlan::LocalBasicCleanup => RuntimeRefinement::LocalBasicCleanup,
            RefinementPlan::AiPostprocessing(plan) => RuntimeRefinement::Ai {
                fallback_to_local_basic_cleanup: plan.fallback_to_local_basic_cleanup,
                processor: AiProcessor::Plan(plan),
            },
        };

        Ok(Self {
            asr,
            refinement,
            locale: plan.locale,
        })
    }

    /// Recognizes one audio input and applies the configured refinement path.
    pub fn process(&self, audio: AudioInput) -> Result<SpeechOutcome, OrallyError> {
        let transcript = self.asr.transcribe(audio)?;
        let raw_transcript = transcript.clone();

        let processed = match &self.refinement {
            RuntimeRefinement::Raw => ProcessedText {
                text: transcript.text,
                changes: Vec::new(),
            },
            RuntimeRefinement::LocalBasicCleanup => {
                run_local_cleanup(process_input(transcript, &self.locale))?
            }
            RuntimeRefinement::Ai {
                processor,
                fallback_to_local_basic_cleanup,
            } => run_ai_refinement(
                processor,
                process_input(transcript, &self.locale),
                *fallback_to_local_basic_cleanup,
            )?,
        };

        Ok(SpeechOutcome {
            raw_transcript,
            final_text: processed.text,
            changes: processed.changes,
        })
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
        };

        Self {
            asr,
            refinement,
            locale: locale.into(),
        }
    }
}

fn build_asr_provider(plan: RecognitionPlan) -> Result<Box<dyn AsrProvider>, SpeechBuildError> {
    let api_key = plan.credential.resolve().ok_or_else(|| {
        SpeechBuildError::MissingRecognitionCredential {
            environment_variable: plan.credential.environment_variable().to_string(),
        }
    })?;

    match plan.protocol.resolve(&plan.base_url, &plan.model) {
        AsrProtocol::OpenAiTranscriptions => {
            let mut config = OpenAiCompatibleAsrConfig::new(plan.base_url, api_key, plan.model);
            config.language = plan.language;
            config.prompt = plan.prompt;
            OpenAiCompatibleAsrProvider::new(config)
                .map(|provider| Box::new(provider) as Box<dyn AsrProvider>)
                .map_err(SpeechBuildError::RecognitionAdapter)
        }
        AsrProtocol::ChatAudio => {
            let mut config = ChatAudioAsrConfig::new(plan.base_url, api_key, plan.model);
            config.language = plan.language;
            config.prompt = plan.prompt;
            ChatAudioAsrProvider::new(config)
                .map(|provider| Box::new(provider) as Box<dyn AsrProvider>)
                .map_err(SpeechBuildError::RecognitionAdapter)
        }
        AsrProtocol::Auto => unreachable!("auto protocol resolves before provider construction"),
    }
}

fn run_ai_refinement(
    source: &AiProcessor,
    input: ProcessInput,
    fallback_to_local_basic_cleanup: bool,
) -> Result<ProcessedText, OrallyError> {
    match source {
        AiProcessor::Plan(plan) => {
            let processor = build_ai_processor(plan)?;
            apply_ai_processor(&processor, input, fallback_to_local_basic_cleanup)
        }
        #[cfg(test)]
        AiProcessor::Adapter(processor) => {
            apply_ai_processor(processor.as_ref(), input, fallback_to_local_basic_cleanup)
        }
    }
}

fn build_ai_processor(plan: &AiPostprocessPlan) -> Result<OpenAiChatPostprocessor, OrallyError> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

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
                prompt: None,
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
                    prompt: None,
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
    fn protocol_aliases_and_auto_inference_match_existing_behavior() {
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
        assert_eq!(
            AsrProtocol::Auto.resolve("https://openrouter.ai/api/v1", "model"),
            AsrProtocol::ChatAudio
        );
        assert_eq!(
            AsrProtocol::Auto.resolve("https://example.maas.aliyuncs.com/v1", "model"),
            AsrProtocol::ChatAudio
        );
        assert_eq!(
            AsrProtocol::Auto.resolve("https://example.com/v1", "qwen3-asr-flash"),
            AsrProtocol::ChatAudio
        );
        assert_eq!(
            AsrProtocol::Auto.resolve("https://api.openai.com/v1", "whisper-1"),
            AsrProtocol::OpenAiTranscriptions
        );
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
