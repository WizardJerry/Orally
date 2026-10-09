use orally_core::{OrallyError, ProcessInput, ProcessedText, TextProcessor};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiChatPostprocessorConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub system_prompt: String,
    pub user_template: String,
    pub timeout: Duration,
}

impl OpenAiChatPostprocessorConfig {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            system_prompt: default_system_prompt(),
            user_template: default_user_template(),
            timeout: Duration::from_secs(120),
        }
    }

    pub fn endpoint(&self) -> String {
        chat_completions_endpoint(&self.base_url)
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiChatPostprocessor {
    config: OpenAiChatPostprocessorConfig,
    client: Client,
}

impl OpenAiChatPostprocessor {
    pub fn new(config: OpenAiChatPostprocessorConfig) -> Result<Self, OrallyError> {
        validate_config(&config)?;
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|error| OrallyError::Processing(error.to_string()))?;

        Ok(Self { config, client })
    }
}

impl TextProcessor for OpenAiChatPostprocessor {
    fn process(&self, input: ProcessInput) -> Result<ProcessedText, OrallyError> {
        let user_prompt = render_user_template(&self.config.user_template, &input);
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: self.config.system_prompt.clone(),
                },
                ChatMessage {
                    role: "user",
                    content: user_prompt,
                },
            ],
            temperature: 0.1,
            stream: false,
        };

        let response = self
            .client
            .post(self.config.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .map_err(|error| OrallyError::Processing(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            return Err(OrallyError::Processing(format!(
                "postprocessor returned HTTP {status}: {body}"
            )));
        }

        let body = response
            .json::<ChatCompletionResponse>()
            .map_err(|error| OrallyError::Processing(error.to_string()))?;
        let text = body
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .map(sanitize_model_text)
            .unwrap_or_default();

        if text.is_empty() {
            return Err(OrallyError::Processing(
                "postprocessor returned empty text".to_string(),
            ));
        }

        Ok(ProcessedText {
            text,
            changes: vec!["processed by AI postprocessor".to_string()],
        })
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

pub fn default_system_prompt() -> String {
    "You are Orally's AI postprocessor for raw speech-to-text transcripts. Produce text that is ready to paste into the user's active app. Preserve the speaker's meaning, intent, language, names, product terms, URLs, and code identifiers. Remove filler words, repeated fragments, false starts, and self-corrections unless they change the meaning. Add only punctuation and lightweight structure that are clearly implied by the transcript. Do not invent facts, explanations, headings, labels, quotes, or markdown fences. Return only the final text.".to_string()
}

pub fn default_user_template() -> String {
    "Locale: {{locale}}\nTask: cleanup\nTranscript:\n{{transcript}}\n\nClean the transcript into polished text in the original language. Keep normal prose unless the speaker explicitly asks for a list, translation, or another format.".to_string()
}

pub fn render_user_template(template: &str, input: &ProcessInput) -> String {
    template
        .replace("{{transcript}}", &input.transcript.text)
        .replace("{{locale}}", &input.context.locale)
}

fn validate_config(config: &OpenAiChatPostprocessorConfig) -> Result<(), OrallyError> {
    if config.base_url.trim().is_empty() {
        return Err(OrallyError::InvalidInput(
            "postprocess base URL is required".to_string(),
        ));
    }

    if config.api_key.trim().is_empty() {
        return Err(OrallyError::InvalidInput(
            "postprocess API key is required".to_string(),
        ));
    }

    if config.model.trim().is_empty() {
        return Err(OrallyError::InvalidInput(
            "postprocess model is required".to_string(),
        ));
    }

    if !matches!(reqwest::Url::parse(config.base_url.trim()), Ok(url) if matches!(url.scheme(), "http" | "https"))
    {
        return Err(OrallyError::InvalidInput(
            "postprocess base URL must be a valid HTTP or HTTPS URL".to_string(),
        ));
    }

    Ok(())
}

fn chat_completions_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim();
    let Ok(mut url) = reqwest::Url::parse(trimmed) else {
        return trimmed.to_string();
    };
    let path = url.path().trim_end_matches('/');
    let base_path = path
        .strip_suffix("/audio/transcriptions")
        .or_else(|| path.strip_suffix("/chat/completions"))
        .unwrap_or(path);
    url.set_path(&format!("{base_path}/chat/completions"));
    url.set_fragment(None);
    url.to_string()
}

fn sanitize_model_text(text: &str) -> String {
    let mut cleaned = text.trim();

    if cleaned.starts_with("```") {
        cleaned = cleaned.trim_start_matches('`').trim();
        if let Some(rest) = cleaned.strip_prefix("text") {
            cleaned = rest.trim_start_matches(['\r', '\n', ' ']);
        }
        if let Some(rest) = cleaned.strip_prefix("markdown") {
            cleaned = rest.trim_start_matches(['\r', '\n', ' ']);
        }
        cleaned = cleaned.trim_end_matches('`').trim();
    }

    for prefix in [
        "Final text:",
        "Final:",
        "Output:",
        "Text:",
        "Result:",
        "最终文本：",
        "输出：",
        "结果：",
    ] {
        if let Some(rest) = cleaned.strip_prefix(prefix) {
            cleaned = rest.trim();
            break;
        }
    }

    let mut unquoted = cleaned;
    if let Some(value) = unquoted
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        unquoted = value;
    }
    if let Some(value) = unquoted
        .strip_prefix('“')
        .and_then(|value| value.strip_suffix('”'))
    {
        unquoted = value;
    }

    unquoted.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use orally_core::{AppContext, PostprocessPrompt, Transcript};

    #[test]
    fn renders_prompt_template() {
        let input = ProcessInput {
            transcript: Transcript {
                text: "hello world".to_string(),
                language: None,
                segments: Vec::new(),
            },
            context: AppContext {
                locale: "en-US".to_string(),
                ..AppContext::default()
            },
            prompt: PostprocessPrompt::default(),
            dictionary_terms: Vec::new(),
        };

        let rendered = render_user_template("{{locale}}: {{transcript}}", &input);

        assert_eq!(rendered, "en-US: hello world");
    }

    #[test]
    fn builds_chat_endpoint() {
        let config = OpenAiChatPostprocessorConfig::new("https://api.example.com/v1", "key", "m");

        assert_eq!(
            config.endpoint(),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn chat_endpoint_accepts_complete_api_routes_and_preserves_gateway_query() {
        for base_url in [
            "  https://api.example.com/gateway/v1/audio/transcriptions/?tenant=a%2Fb#ignored  ",
            "https://api.example.com/gateway/v1/chat/completions/?tenant=a%2Fb#ignored",
            " https://api.example.com/gateway/v1///?tenant=a%2Fb#ignored ",
        ] {
            let config = OpenAiChatPostprocessorConfig::new(base_url, "key", "model");
            assert_eq!(
                config.endpoint(),
                "https://api.example.com/gateway/v1/chat/completions?tenant=a%2Fb"
            );
        }
    }

    #[test]
    fn postprocess_rejects_invalid_service_urls_during_construction() {
        for base_url in [
            "not a URL?secret=fixture-secret",
            "ftp://api.example.com/v1",
            "https://",
        ] {
            let error = OpenAiChatPostprocessor::new(OpenAiChatPostprocessorConfig::new(
                base_url,
                "fixture-key",
                "model",
            ))
            .unwrap_err();
            assert!(matches!(&error, OrallyError::InvalidInput(_)));
            assert!(error.to_string().contains("valid HTTP or HTTPS URL"));
            assert!(!error.to_string().contains("fixture-secret"));
            assert!(!error.to_string().contains("fixture-key"));
        }
    }

    #[test]
    fn sanitizes_common_model_wrappers() {
        assert_eq!(
            sanitize_model_text("Final text: \"hello world\""),
            "hello world"
        );
        assert_eq!(
            sanitize_model_text("```text\nhello world\n```"),
            "hello world"
        );
        assert_eq!(sanitize_model_text("输出：你好世界"), "你好世界");
    }
}
