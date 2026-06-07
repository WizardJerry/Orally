use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioFormat {
    Pcm16,
    Wav,
    Opus,
    DemoText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioInput {
    pub bytes: Vec<u8>,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub format: AudioFormat,
}

impl AudioInput {
    pub fn demo_text(text: impl Into<String>) -> Self {
        Self {
            bytes: text.into().into_bytes(),
            sample_rate_hz: 16_000,
            channels: 1,
            format: AudioFormat::DemoText,
        }
    }

    pub fn wav(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            sample_rate_hz: 0,
            channels: 0,
            format: AudioFormat::Wav,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppContext {
    pub app_id: Option<String>,
    pub field_role: Option<String>,
    pub locale: String,
    pub style: OutputStyle,
}

impl Default for AppContext {
    fn default() -> Self {
        Self {
            app_id: None,
            field_role: None,
            locale: "zh-CN".to_string(),
            style: OutputStyle::Natural,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStyle {
    Natural,
    Formal,
    Concise,
    Technical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryTerm {
    pub spoken: String,
    pub written: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostprocessPrompt {
    pub name: String,
    pub system_prompt: String,
    pub user_template: String,
}

impl Default for PostprocessPrompt {
    fn default() -> Self {
        Self {
            name: "default-cleanup".to_string(),
            system_prompt: "Clean ASR text while preserving user intent.".to_string(),
            user_template: "{{transcript}}".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessInput {
    pub transcript: Transcript,
    pub context: AppContext,
    pub prompt: PostprocessPrompt,
    pub dictionary_terms: Vec<DictionaryTerm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessedText {
    pub text: String,
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertMode {
    Direct,
    ClipboardFallback,
    PreviewOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrallyError {
    InvalidInput(String),
    Audio(String),
    Asr(String),
    Processing(String),
    Insertion(String),
}

impl Display for OrallyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OrallyError::InvalidInput(message) => write!(f, "invalid input: {message}"),
            OrallyError::Audio(message) => write!(f, "audio failed: {message}"),
            OrallyError::Asr(message) => write!(f, "asr failed: {message}"),
            OrallyError::Processing(message) => write!(f, "processing failed: {message}"),
            OrallyError::Insertion(message) => write!(f, "insertion failed: {message}"),
        }
    }
}

impl Error for OrallyError {}

pub trait AsrProvider {
    fn transcribe(&self, audio: AudioInput) -> Result<Transcript, OrallyError>;
}

pub trait TextProcessor {
    fn process(&self, input: ProcessInput) -> Result<ProcessedText, OrallyError>;
}

pub trait TextInserter {
    fn insert(&self, text: &str, mode: InsertMode) -> Result<(), OrallyError>;
}

pub struct OrallyPipeline<A, P, I> {
    asr: A,
    processor: P,
    inserter: I,
}

impl<A, P, I> OrallyPipeline<A, P, I>
where
    A: AsrProvider,
    P: TextProcessor,
    I: TextInserter,
{
    pub fn new(asr: A, processor: P, inserter: I) -> Self {
        Self {
            asr,
            processor,
            inserter,
        }
    }

    pub fn run(
        &self,
        audio: AudioInput,
        context: AppContext,
        prompt: PostprocessPrompt,
        dictionary_terms: Vec<DictionaryTerm>,
        insert_mode: InsertMode,
    ) -> Result<ProcessedText, OrallyError> {
        let transcript = self.asr.transcribe(audio)?;
        let processed = self.processor.process(ProcessInput {
            transcript,
            context,
            prompt,
            dictionary_terms,
        })?;

        if insert_mode != InsertMode::PreviewOnly {
            self.inserter.insert(&processed.text, insert_mode)?;
        }

        Ok(processed)
    }
}

#[derive(Debug, Default)]
pub struct DemoTextAsrProvider;

impl AsrProvider for DemoTextAsrProvider {
    fn transcribe(&self, audio: AudioInput) -> Result<Transcript, OrallyError> {
        if audio.format != AudioFormat::DemoText {
            return Err(OrallyError::Asr(
                "demo provider only accepts DemoText input".to_string(),
            ));
        }

        let text = String::from_utf8(audio.bytes)
            .map_err(|error| OrallyError::InvalidInput(error.to_string()))?;

        Ok(Transcript {
            text,
            language: None,
            segments: Vec::new(),
        })
    }
}

#[derive(Debug, Default)]
pub struct BuiltInTextProcessor;

impl TextProcessor for BuiltInTextProcessor {
    fn process(&self, input: ProcessInput) -> Result<ProcessedText, OrallyError> {
        let mut changes = Vec::new();
        let original = input.transcript.text;
        let mut text = normalize_spaces(&original);

        if text != original {
            changes.push("normalized whitespace".to_string());
        }

        let without_fillers = remove_fillers(&text);
        if without_fillers != text {
            changes.push("removed common filler words".to_string());
            text = without_fillers;
        }

        for term in input.dictionary_terms {
            if term.spoken.is_empty() {
                continue;
            }

            let replaced = text.replace(&term.spoken, &term.written);
            if replaced != text {
                changes.push(format!("applied dictionary term: {}", term.written));
                text = replaced;
            }
        }

        let punctuated = ensure_sentence_end(&text, &input.context.locale);
        if punctuated != text {
            changes.push("added terminal punctuation".to_string());
            text = punctuated;
        }

        Ok(ProcessedText { text, changes })
    }
}

#[derive(Debug, Default)]
pub struct MemoryInserter {
    last_text: Mutex<Option<String>>,
}

impl MemoryInserter {
    pub fn last_text(&self) -> Option<String> {
        self.last_text.lock().ok().and_then(|guard| guard.clone())
    }
}

impl TextInserter for MemoryInserter {
    fn insert(&self, text: &str, _mode: InsertMode) -> Result<(), OrallyError> {
        let mut guard = self
            .last_text
            .lock()
            .map_err(|error| OrallyError::Insertion(error.to_string()))?;
        *guard = Some(text.to_string());
        Ok(())
    }
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn remove_fillers(text: &str) -> String {
    let fillers = ["嗯", "呃", "那个", "就是", "uh", "um", "er"];
    let mut cleaned = text.to_string();

    for filler in fillers {
        cleaned = cleaned
            .split_whitespace()
            .filter(|part| *part != filler)
            .collect::<Vec<_>>()
            .join(" ");
    }

    cleaned
}

fn ensure_sentence_end(text: &str, locale: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || ends_with_terminal_punctuation(trimmed) {
        return trimmed.to_string();
    }

    let punctuation = if locale.starts_with("zh") { "。" } else { "." };
    format!("{trimmed}{punctuation}")
}

fn ends_with_terminal_punctuation(text: &str) -> bool {
    text.ends_with('.')
        || text.ends_with('!')
        || text.ends_with('?')
        || text.ends_with('。')
        || text.ends_with('！')
        || text.ends_with('？')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_cleans_demo_transcript() {
        let pipeline = OrallyPipeline::new(
            DemoTextAsrProvider,
            BuiltInTextProcessor,
            MemoryInserter::default(),
        );

        let result = pipeline
            .run(
                AudioInput::demo_text("嗯   今天   写给 visual studio code 团队"),
                AppContext::default(),
                PostprocessPrompt::default(),
                vec![DictionaryTerm {
                    spoken: "visual studio code".to_string(),
                    written: "Visual Studio Code".to_string(),
                }],
                InsertMode::Direct,
            )
            .expect("pipeline should process demo input");

        assert_eq!(result.text, "今天 写给 Visual Studio Code 团队。");
        assert!(result
            .changes
            .iter()
            .any(|change| change.contains("dictionary")));
    }

    #[test]
    fn english_locale_uses_english_period() {
        let processor = BuiltInTextProcessor;
        let result = processor
            .process(ProcessInput {
                transcript: Transcript {
                    text: "um hello world".to_string(),
                    language: Some("en".to_string()),
                    segments: Vec::new(),
                },
                context: AppContext {
                    locale: "en-US".to_string(),
                    ..AppContext::default()
                },
                prompt: PostprocessPrompt::default(),
                dictionary_terms: Vec::new(),
            })
            .expect("processor should handle English locale");

        assert_eq!(result.text, "hello world.");
    }
}
