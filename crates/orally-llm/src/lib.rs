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
            .unwrap_or("")
            .trim()
            .to_string();

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
    "You are Orally's dictation postprocessor. Clean speech-to-text output while preserving the user's meaning. Return only the final text, with no explanations, markdown, quotes, or labels.".to_string()
}

pub fn default_user_template() -> String {
    "Locale: {{locale}}\nTranscript:\n{{transcript}}\n\nRewrite the transcript into polished text suitable for direct insertion.".to_string()
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

    Ok(())
}

fn chat_completions_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
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
}
