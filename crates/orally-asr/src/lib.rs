use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use orally_audio::encode_wav_pcm16;
use orally_core::{AsrProvider, AudioFormat, AudioInput, OrallyError, Transcript};
use reqwest::blocking::{multipart, Client};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleAsrConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub timeout: Duration,
}

impl OpenAiCompatibleAsrConfig {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            language: None,
            prompt: None,
            timeout: Duration::from_secs(120),
        }
    }

    pub fn endpoint(&self) -> String {
        transcription_endpoint(&self.base_url)
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleAsrProvider {
    config: OpenAiCompatibleAsrConfig,
    client: Client,
}

impl OpenAiCompatibleAsrProvider {
    pub fn new(config: OpenAiCompatibleAsrConfig) -> Result<Self, OrallyError> {
        validate_config(&config)?;
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|error| OrallyError::Asr(error.to_string()))?;

        Ok(Self { config, client })
    }
}

impl AsrProvider for OpenAiCompatibleAsrProvider {
    fn transcribe(&self, audio: AudioInput) -> Result<Transcript, OrallyError> {
        let wav = audio_to_wav(audio)?;
        let file = multipart::Part::bytes(wav)
            .file_name("orally-input.wav")
            .mime_str("audio/wav")
            .map_err(|error| OrallyError::Asr(error.to_string()))?;

        let mut form = multipart::Form::new()
            .text("model", self.config.model.clone())
            .text("response_format", "json")
            .part("file", file);

        if let Some(language) = &self.config.language {
            form = form.text("language", language.clone());
        }

        if let Some(prompt) = &self.config.prompt {
            form = form.text("prompt", prompt.clone());
        }

        let response = self
            .client
            .post(self.config.endpoint())
            .bearer_auth(&self.config.api_key)
            .multipart(form)
            .send()
            .map_err(|error| OrallyError::Asr(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            return Err(OrallyError::Asr(format!(
                "provider returned HTTP {status}: {body}"
            )));
        }

        let body = response
            .json::<TranscriptionResponse>()
            .map_err(|error| OrallyError::Asr(error.to_string()))?;

        if body.text.trim().is_empty() {
            return Err(OrallyError::Asr(
                "provider returned an empty transcript".to_string(),
            ));
        }

        Ok(Transcript {
            text: body.text,
            language: body.language,
            segments: Vec::new(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
    language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatAudioAsrConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub timeout: Duration,
}

impl ChatAudioAsrConfig {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            language: None,
            prompt: None,
            timeout: Duration::from_secs(120),
        }
    }

    pub fn endpoint(&self) -> String {
        chat_completions_endpoint(&self.base_url)
    }

    fn audio_shape(&self) -> ChatAudioShape {
        let base_url = self.base_url.to_ascii_lowercase();
        if base_url.contains("openrouter.ai") {
            ChatAudioShape::OpenRouter
        } else if base_url.contains("dashscope.aliyuncs.com")
            || base_url.contains("dashscope-intl.aliyuncs.com")
            || base_url.contains("dashscope-us.aliyuncs.com")
        {
            ChatAudioShape::DashScope
        } else {
            ChatAudioShape::GenericSnake
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatAudioAsrProvider {
    config: ChatAudioAsrConfig,
    client: Client,
}

impl ChatAudioAsrProvider {
    pub fn new(config: ChatAudioAsrConfig) -> Result<Self, OrallyError> {
        validate_chat_audio_config(&config)?;
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|error| OrallyError::Asr(error.to_string()))?;

        Ok(Self { config, client })
    }
}

impl AsrProvider for ChatAudioAsrProvider {
    fn transcribe(&self, audio: AudioInput) -> Result<Transcript, OrallyError> {
        let wav = audio_to_wav(audio)?;
        let audio_base64 = BASE64_STANDARD.encode(wav);
        let prompt = chat_audio_prompt(
            self.config.prompt.as_deref(),
            self.config.language.as_deref(),
        );
        let audio_shape = self.config.audio_shape();
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: vec![ChatCompletionMessage {
                role: "user",
                content: chat_content_parts(prompt, audio_base64, audio_shape),
            }],
            stream: false,
        };

        let response = self
            .client
            .post(self.config.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .map_err(|error| OrallyError::Asr(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            return Err(OrallyError::Asr(format!(
                "provider returned HTTP {status}: {body}"
            )));
        }

        let body = response
            .json::<ChatCompletionResponse>()
            .map_err(|error| OrallyError::Asr(error.to_string()))?;
        let text = body
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .trim()
            .to_string();

        if text.is_empty() {
            return Err(OrallyError::Asr(
                "provider returned an empty transcript".to_string(),
            ));
        }

        Ok(Transcript {
            text,
            language: self.config.language.clone(),
            segments: Vec::new(),
        })
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatCompletionMessage>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ChatCompletionMessage {
    role: &'static str,
    content: Vec<ChatContentPart>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ChatContentPart {
    Text {
        #[serde(rename = "type")]
        content_type: &'static str,
        text: String,
    },
    SnakeInputAudio {
        #[serde(rename = "type")]
        content_type: &'static str,
        input_audio: FormattedInputAudio,
    },
    DashScopeInputAudio {
        #[serde(rename = "type")]
        content_type: &'static str,
        input_audio: DataUrlInputAudio,
    },
    CamelInputAudio {
        #[serde(rename = "type")]
        content_type: &'static str,
        #[serde(rename = "inputAudio")]
        input_audio: FormattedInputAudio,
    },
}

#[derive(Debug, Serialize)]
struct FormattedInputAudio {
    data: String,
    format: &'static str,
}

#[derive(Debug, Serialize)]
struct DataUrlInputAudio {
    data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatAudioShape {
    OpenRouter,
    DashScope,
    GenericSnake,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionMessageResponse {
    content: Option<String>,
}

fn validate_config(config: &OpenAiCompatibleAsrConfig) -> Result<(), OrallyError> {
    if config.base_url.trim().is_empty() {
        return Err(OrallyError::InvalidInput(
            "ASR base URL is required".to_string(),
        ));
    }

    if config.api_key.trim().is_empty() {
        return Err(OrallyError::InvalidInput(
            "ASR API key is required".to_string(),
        ));
    }

    if config.model.trim().is_empty() {
        return Err(OrallyError::InvalidInput(
            "ASR model is required".to_string(),
        ));
    }

    Ok(())
}

fn validate_chat_audio_config(config: &ChatAudioAsrConfig) -> Result<(), OrallyError> {
    if config.base_url.trim().is_empty() {
        return Err(OrallyError::InvalidInput(
            "ASR base URL is required".to_string(),
        ));
    }

    if config.api_key.trim().is_empty() {
        return Err(OrallyError::InvalidInput(
            "ASR API key is required".to_string(),
        ));
    }

    if config.model.trim().is_empty() {
        return Err(OrallyError::InvalidInput(
            "ASR model is required".to_string(),
        ));
    }

    Ok(())
}

fn audio_to_wav(audio: AudioInput) -> Result<Vec<u8>, OrallyError> {
    match audio.format {
        AudioFormat::Wav => Ok(audio.bytes),
        AudioFormat::Pcm16 => encode_wav_pcm16(&audio),
        other => Err(OrallyError::InvalidInput(format!(
            "ASR provider requires WAV or PCM16 audio, got {other:?}"
        ))),
    }
}

fn transcription_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/audio/transcriptions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/audio/transcriptions")
    }
}

fn chat_completions_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

fn chat_audio_prompt(prompt: Option<&str>, language: Option<&str>) -> String {
    let mut text = prompt
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Please transcribe this audio exactly. Return only the transcript text.")
        .to_string();

    if let Some(language) = language.filter(|value| !value.trim().is_empty()) {
        text.push_str("\nLanguage hint: ");
        text.push_str(language);
    }

    text
}

fn chat_audio_part(audio_base64: String, shape: ChatAudioShape) -> ChatContentPart {
    match shape {
        ChatAudioShape::OpenRouter => ChatContentPart::CamelInputAudio {
            content_type: "input_audio",
            input_audio: FormattedInputAudio {
                data: audio_base64,
                format: "wav",
            },
        },
        ChatAudioShape::DashScope => ChatContentPart::DashScopeInputAudio {
            content_type: "input_audio",
            input_audio: DataUrlInputAudio {
                data: format!("data:audio/wav;base64,{audio_base64}"),
            },
        },
        ChatAudioShape::GenericSnake => ChatContentPart::SnakeInputAudio {
            content_type: "input_audio",
            input_audio: FormattedInputAudio {
                data: audio_base64,
                format: "wav",
            },
        },
    }
}

fn chat_content_parts(
    prompt: String,
    audio_base64: String,
    shape: ChatAudioShape,
) -> Vec<ChatContentPart> {
    let audio = chat_audio_part(audio_base64, shape);

    if shape == ChatAudioShape::DashScope {
        vec![audio]
    } else {
        vec![
            ChatContentPart::Text {
                text: prompt,
                content_type: "text",
            },
            audio,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_endpoint_from_v1_base_url() {
        let config = OpenAiCompatibleAsrConfig::new("https://api.example.com/v1/", "key", "model");

        assert_eq!(
            config.endpoint(),
            "https://api.example.com/v1/audio/transcriptions"
        );
    }

    #[test]
    fn keeps_explicit_transcription_endpoint() {
        let config = OpenAiCompatibleAsrConfig::new(
            "https://api.example.com/v1/audio/transcriptions",
            "key",
            "model",
        );

        assert_eq!(
            config.endpoint(),
            "https://api.example.com/v1/audio/transcriptions"
        );
    }

    #[test]
    fn builds_chat_completions_endpoint_from_openrouter_base_url() {
        let config = ChatAudioAsrConfig::new("https://openrouter.ai/api/v1", "key", "model");

        assert_eq!(
            config.endpoint(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[test]
    fn keeps_explicit_chat_completions_endpoint() {
        let config = ChatAudioAsrConfig::new(
            "https://openrouter.ai/api/v1/chat/completions",
            "key",
            "model",
        );

        assert_eq!(
            config.endpoint(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[test]
    fn rejects_empty_model() {
        let config = OpenAiCompatibleAsrConfig::new("https://api.example.com/v1", "key", "");

        assert!(OpenAiCompatibleAsrProvider::new(config).is_err());
    }

    #[test]
    fn openrouter_chat_audio_uses_camel_case_audio_field() {
        let part = chat_audio_part("abc".to_string(), ChatAudioShape::OpenRouter);
        let value = serde_json::to_value(part).expect("part should serialize");

        assert!(value.get("inputAudio").is_some());
        assert!(value.get("input_audio").is_none());
    }

    #[test]
    fn generic_chat_audio_uses_snake_case_audio_field() {
        let part = chat_audio_part("abc".to_string(), ChatAudioShape::GenericSnake);
        let value = serde_json::to_value(part).expect("part should serialize");

        assert!(value.get("input_audio").is_some());
        assert!(value.get("inputAudio").is_none());
    }

    #[test]
    fn dashscope_chat_audio_uses_data_url_without_format_field() {
        let part = chat_audio_part("abc".to_string(), ChatAudioShape::DashScope);
        let value = serde_json::to_value(part).expect("part should serialize");
        let input_audio = value
            .get("input_audio")
            .expect("DashScope should use input_audio");

        assert_eq!(
            input_audio.get("data").and_then(|data| data.as_str()),
            Some("data:audio/wav;base64,abc")
        );
        assert!(input_audio.get("format").is_none());
    }

    #[test]
    fn dashscope_base_url_uses_dashscope_audio_shape() {
        let config = ChatAudioAsrConfig::new(
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "key",
            "qwen3-asr-flash",
        );

        assert_eq!(config.audio_shape(), ChatAudioShape::DashScope);
    }

    #[test]
    fn dashscope_chat_audio_sends_only_audio_content() {
        let parts = chat_content_parts(
            "Please transcribe".to_string(),
            "abc".to_string(),
            ChatAudioShape::DashScope,
        );
        let value = serde_json::to_value(parts).expect("parts should serialize");
        let items = value.as_array().expect("parts should be an array");

        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].get("type").and_then(|value| value.as_str()),
            Some("input_audio")
        );
    }

    #[test]
    fn openrouter_chat_audio_keeps_text_prompt() {
        let parts = chat_content_parts(
            "Please transcribe".to_string(),
            "abc".to_string(),
            ChatAudioShape::OpenRouter,
        );
        let value = serde_json::to_value(parts).expect("parts should serialize");
        let items = value.as_array().expect("parts should be an array");

        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].get("type").and_then(|value| value.as_str()),
            Some("text")
        );
    }
}
