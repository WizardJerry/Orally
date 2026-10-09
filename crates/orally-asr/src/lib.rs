use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use orally_audio::{decode_wav_pcm16, encode_wav_pcm16, split_pcm16_audio};
use orally_core::{AsrProvider, AudioFormat, AudioInput, OrallyError, Transcript};
use reqwest::blocking::{multipart, Client};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_TRANSCRIPTION_CHUNK_DURATION: Duration = Duration::from_secs(60);

enum RequestFailure {
    Http {
        status: reqwest::StatusCode,
        body: String,
    },
    Other(OrallyError),
}

impl From<OrallyError> for RequestFailure {
    fn from(error: OrallyError) -> Self {
        Self::Other(error)
    }
}

impl From<RequestFailure> for OrallyError {
    fn from(error: RequestFailure) -> Self {
        match error {
            RequestFailure::Http { status, body } => {
                Self::Asr(format!("provider returned HTTP {status}: {body}"))
            }
            RequestFailure::Other(error) => error,
        }
    }
}

fn successful_response(
    response: reqwest::blocking::Response,
) -> Result<reqwest::blocking::Response, RequestFailure> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response
        .text()
        .unwrap_or_else(|error| format!("could not read provider error response body: {error}"));
    Err(RequestFailure::Http { status, body })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleAsrConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub language: Option<String>,
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
        let chunks = audio_to_wav_chunks(audio)?;
        let mut transcripts = Vec::with_capacity(chunks.len());

        for wav in chunks {
            transcripts.push(self.transcribe_wav(wav)?);
        }

        combine_transcripts(transcripts, self.config.language.clone())
    }
}

impl OpenAiCompatibleAsrProvider {
    fn transcribe_wav(&self, wav: Vec<u8>) -> Result<Transcript, OrallyError> {
        self.request_wav(wav).map_err(OrallyError::from)
    }

    fn request_wav(&self, wav: Vec<u8>) -> Result<Transcript, RequestFailure> {
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

        let mut request = self
            .client
            .post(self.config.endpoint())
            .bearer_auth(&self.config.api_key)
            .multipart(form)
            .build()
            .map_err(|error| OrallyError::Asr(error.to_string()))?;
        // Blocking multipart otherwise streams through a sender that can fail
        // before an early HTTP rejection is returned. WAV chunks are already in
        // memory; buffer the form too so HTTP status can drive negotiation.
        if let Some(body) = request.body_mut() {
            body.buffer()
                .map_err(|error| OrallyError::Asr(error.to_string()))?;
        }
        let response = self
            .client
            .execute(request)
            .map_err(|error| OrallyError::Asr(error.to_string()))?;

        let response = successful_response(response)?;

        let body = response
            .json::<TranscriptionResponse>()
            .map_err(|error| OrallyError::Asr(error.to_string()))?;

        if body.text.trim().is_empty() {
            return Err(
                OrallyError::Asr("provider returned an empty transcript".to_string()).into(),
            );
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
            timeout: Duration::from_secs(120),
        }
    }

    pub fn endpoint(&self) -> String {
        chat_completions_endpoint(&self.base_url)
    }
}

#[derive(Debug, Clone)]
pub struct ChatAudioAsrProvider {
    config: ChatAudioAsrConfig,
    client: Client,
}

#[derive(Clone, Copy)]
enum ChatAudioFormat {
    Standard,
    DataUrl,
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
        let chunks = audio_to_wav_chunks(audio)?;
        let mut transcripts = Vec::with_capacity(chunks.len());

        for wav in chunks {
            transcripts.push(self.transcribe_wav(wav)?);
        }

        combine_transcripts(transcripts, self.config.language.clone())
    }
}

impl ChatAudioAsrProvider {
    fn transcribe_wav(&self, wav: Vec<u8>) -> Result<Transcript, OrallyError> {
        self.request_wav(wav, ChatAudioFormat::Standard)
            .map_err(OrallyError::from)
    }

    fn request_wav(
        &self,
        wav: Vec<u8>,
        format: ChatAudioFormat,
    ) -> Result<Transcript, RequestFailure> {
        let audio_base64 = BASE64_STANDARD.encode(wav);
        let messages = match format {
            ChatAudioFormat::Standard => vec![ChatCompletionMessage {
                role: "user",
                content: ChatMessageContent::Parts(chat_content_parts(
                    chat_audio_instruction(self.config.language.as_deref()),
                    audio_base64,
                )),
            }],
            ChatAudioFormat::DataUrl => {
                let mut messages = Vec::new();
                if let Some(context) = audio_context(self.config.language.as_deref()) {
                    messages.push(ChatCompletionMessage {
                        role: "system",
                        content: ChatMessageContent::Text(context),
                    });
                }
                messages.push(ChatCompletionMessage {
                    role: "user",
                    content: ChatMessageContent::Parts(vec![ChatContentPart::DataUrlAudio {
                        content_type: "input_audio",
                        input_audio: DataUrlAudio {
                            data: format!("data:audio/wav;base64,{audio_base64}"),
                        },
                    }]),
                });
                messages
            }
        };
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages,
            stream: false,
        };

        let response = self
            .client
            .post(self.config.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .map_err(|error| OrallyError::Asr(error.to_string()))?;

        let response = successful_response(response)?;

        let body = response
            .json::<ChatCompletionResponse>()
            .map_err(|error| OrallyError::Asr(error.to_string()))?;
        let text = body
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .ok_or_else(|| OrallyError::Asr("provider returned an invalid transcript response: expected a choice with string content".to_string()))?
            .trim()
            .to_string();

        if text.is_empty() {
            return Err(
                OrallyError::Asr("provider returned an empty transcript".to_string()).into(),
            );
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
    content: ChatMessageContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ChatMessageContent {
    Text(String),
    Parts(Vec<ChatContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ChatContentPart {
    Text {
        #[serde(rename = "type")]
        content_type: &'static str,
        text: String,
    },
    InputAudio {
        #[serde(rename = "type")]
        content_type: &'static str,
        input_audio: FormattedInputAudio,
    },
    DataUrlAudio {
        #[serde(rename = "type")]
        content_type: &'static str,
        input_audio: DataUrlAudio,
    },
}

#[derive(Debug, Serialize)]
struct FormattedInputAudio {
    data: String,
    format: &'static str,
}

#[derive(Debug, Serialize)]
struct DataUrlAudio {
    data: String,
}

#[derive(Debug, Clone, Copy)]
enum AudioRequestFormat {
    Multipart,
    ChatDataUrl,
    ChatStandard,
}

impl AudioRequestFormat {
    fn alternative(self, failure: &RequestFailure) -> Option<Self> {
        let RequestFailure::Http { status, .. } = failure else {
            return None;
        };
        match (self, status.as_u16()) {
            (Self::Multipart, 404 | 405) => Some(Self::ChatDataUrl),
            (Self::ChatDataUrl, 400 | 422) => Some(Self::ChatStandard),
            _ => None,
        }
    }
}

/// Negotiates compatible request formats from HTTP responses without guessing from service metadata.
#[derive(Debug, Clone)]
pub struct AutoAsrProvider {
    transcriptions: OpenAiCompatibleAsrProvider,
    chat: ChatAudioAsrProvider,
    selected_format: Arc<Mutex<Option<AudioRequestFormat>>>,
}

impl AutoAsrProvider {
    pub fn new(config: OpenAiCompatibleAsrConfig) -> Result<Self, OrallyError> {
        let transcriptions = OpenAiCompatibleAsrProvider::new(config)?;
        let config = &transcriptions.config;
        let chat = ChatAudioAsrProvider {
            config: ChatAudioAsrConfig {
                base_url: config.base_url.clone(),
                api_key: config.api_key.clone(),
                model: config.model.clone(),
                language: config.language.clone(),
                timeout: config.timeout,
            },
            client: transcriptions.client.clone(),
        };
        Ok(Self {
            transcriptions,
            chat,
            selected_format: Arc::new(Mutex::new(None)),
        })
    }
}

impl AsrProvider for AutoAsrProvider {
    fn transcribe(&self, audio: AudioInput) -> Result<Transcript, OrallyError> {
        let chunks = audio_to_wav_chunks(audio)?;
        // Serialize this provider's requests, including chunks, so concurrent calls share one selected format.
        let mut selected = self.selected_format.lock().map_err(|_| {
            OrallyError::Asr("ASR protocol negotiation state is unavailable".to_string())
        })?;
        let mut format = selected.unwrap_or(AudioRequestFormat::Multipart);
        let mut transcripts = Vec::with_capacity(chunks.len());
        for wav in chunks {
            loop {
                let result = match format {
                    AudioRequestFormat::Multipart => self.transcriptions.request_wav(wav.clone()),
                    AudioRequestFormat::ChatDataUrl => {
                        self.chat.request_wav(wav.clone(), ChatAudioFormat::DataUrl)
                    }
                    AudioRequestFormat::ChatStandard => self
                        .chat
                        .request_wav(wav.clone(), ChatAudioFormat::Standard),
                };
                match result {
                    Ok(transcript) => {
                        *selected = Some(format);
                        transcripts.push(transcript);
                        break;
                    }
                    Err(failure) => {
                        if let Some(alternative) = format.alternative(&failure) {
                            format = alternative;
                        } else {
                            return Err(failure.into());
                        }
                    }
                }
            }
        }
        combine_transcripts(transcripts, self.transcriptions.config.language.clone())
    }
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

    validate_base_url(&config.base_url)?;

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

    validate_base_url(&config.base_url)?;

    Ok(())
}

fn validate_base_url(base_url: &str) -> Result<(), OrallyError> {
    if matches!(reqwest::Url::parse(base_url.trim()), Ok(url) if matches!(url.scheme(), "http" | "https"))
    {
        Ok(())
    } else {
        Err(OrallyError::InvalidInput(
            "ASR base URL must be a valid HTTP or HTTPS URL".to_string(),
        ))
    }
}

fn audio_to_wav_chunks(audio: AudioInput) -> Result<Vec<Vec<u8>>, OrallyError> {
    match audio.format {
        AudioFormat::Wav => match decode_wav_pcm16(&audio.bytes) {
            Ok(decoded) => pcm16_to_wav_chunks(&decoded),
            Err(_) => Ok(vec![audio.bytes]),
        },
        AudioFormat::Pcm16 => pcm16_to_wav_chunks(&audio),
        other => Err(OrallyError::InvalidInput(format!(
            "ASR provider requires WAV or PCM16 audio, got {other:?}"
        ))),
    }
}

fn pcm16_to_wav_chunks(audio: &AudioInput) -> Result<Vec<Vec<u8>>, OrallyError> {
    split_pcm16_audio(audio, MAX_TRANSCRIPTION_CHUNK_DURATION)?
        .iter()
        .map(encode_wav_pcm16)
        .collect()
}

fn combine_transcripts(
    transcripts: Vec<Transcript>,
    fallback_language: Option<String>,
) -> Result<Transcript, OrallyError> {
    let language = transcripts
        .iter()
        .find_map(|transcript| transcript.language.clone())
        .or(fallback_language);
    let text = transcripts
        .into_iter()
        .map(|transcript| transcript.text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if text.trim().is_empty() {
        return Err(OrallyError::Asr(
            "provider returned an empty transcript".to_string(),
        ));
    }

    Ok(Transcript {
        text,
        language,
        segments: Vec::new(),
    })
}

fn transcription_endpoint(base_url: &str) -> String {
    api_endpoint(base_url, "/audio/transcriptions")
}

fn chat_completions_endpoint(base_url: &str) -> String {
    api_endpoint(base_url, "/chat/completions")
}

fn api_endpoint(base_url: &str, endpoint: &str) -> String {
    let trimmed = base_url.trim();
    let Ok(mut url) = reqwest::Url::parse(trimmed) else {
        return trimmed.to_string();
    };
    let path = url.path().trim_end_matches('/');
    let base_path = path
        .strip_suffix("/audio/transcriptions")
        .or_else(|| path.strip_suffix("/chat/completions"))
        .unwrap_or(path);
    url.set_path(&format!("{base_path}{endpoint}"));
    url.set_fragment(None);
    url.to_string()
}

fn chat_audio_instruction(language: Option<&str>) -> String {
    let mut text =
        "Please transcribe this audio exactly. Return only the transcript text.".to_string();

    if let Some(language) = language.filter(|value| !value.trim().is_empty()) {
        text.push_str("\nLanguage hint: ");
        text.push_str(language);
    }

    text
}

fn audio_context(language: Option<&str>) -> Option<String> {
    language
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|language| format!("Language hint: {language}"))
}

fn chat_content_parts(instruction: String, audio_base64: String) -> Vec<ChatContentPart> {
    vec![
        ChatContentPart::Text {
            text: instruction,
            content_type: "text",
        },
        ChatContentPart::InputAudio {
            content_type: "input_audio",
            input_audio: FormattedInputAudio {
                data: audio_base64,
                format: "wav",
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Instant;

    mod early_response;

    fn read_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0, "request ended before its body was complete");
            request.extend_from_slice(&buffer[..count]);
            if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if request.len() >= end + 4 + length {
                    return request;
                }
            }
        }
    }

    fn serve_script(
        responses: Vec<(&'static str, &'static str)>,
    ) -> (String, thread::JoinHandle<Vec<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            loop {
                let timeout = if requests.is_empty() {
                    Duration::from_secs(5)
                } else {
                    Duration::from_millis(300)
                };
                let deadline = Instant::now() + timeout;
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
                let (status, body) = responses.get(requests.len()).copied().unwrap_or((
                    "500 Internal Server Error",
                    "{\"error\":\"unexpected request\"}",
                ));
                requests.push(read_request(&mut stream));
                if !status.is_empty() {
                    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
            }
        });
        (base_url, server)
    }

    fn auto_provider(base_url: String) -> AutoAsrProvider {
        let mut config =
            OpenAiCompatibleAsrConfig::new(base_url, "fixture-key", "audio-transcriber");
        config.timeout = Duration::from_secs(2);
        AutoAsrProvider::new(config).unwrap()
    }

    fn request_json(request: &[u8]) -> serde_json::Value {
        let end = request
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap();
        serde_json::from_slice(&request[end + 4..]).unwrap()
    }

    #[test]
    fn automatic_chat_recognition_rejects_missing_content_instead_of_reporting_empty_speech() {
        for (body, explicitly_empty) in [
            ("{\"choices\":[]}", false),
            ("{\"choices\":[{\"message\":{\"content\":null}}]}", false),
            ("{\"choices\":[{\"message\":{}}]}", false),
            ("{\"choices\":[{\"message\":{\"content\":7}}]}", false),
            ("{\"unexpected\":true}", false),
            ("{\"choices\":[{\"message\":{\"content\":\"\"}}]}", true),
        ] {
            let (base_url, server) = serve_script(vec![
                ("404 Not Found", "{\"error\":\"route unavailable\"}"),
                ("200 OK", body),
            ]);
            let result = auto_provider(base_url).transcribe(fixture_audio());
            let requests = server.join().unwrap();
            let error = result.unwrap_err();
            assert_eq!(
                requests.len(),
                2,
                "successful malformed/empty responses must not negotiate another format"
            );
            assert_eq!(
                error == OrallyError::Asr("provider returned an empty transcript".to_string()),
                explicitly_empty,
                "only an explicitly empty content string represents silent audio"
            );
        }
        for body in [
            "{\"unexpected\":true}",
            "{\"text\":\"\"}",
            "{\"text\":null}",
        ] {
            let (base_url, server) = serve_script(vec![("200 OK", body)]);
            let result = auto_provider(base_url).transcribe(fixture_audio());
            let requests = server.join().unwrap();
            assert!(result.is_err());
            assert_eq!(
                requests.len(),
                1,
                "multipart successful malformed/empty responses must not switch to chat"
            );
        }
    }

    fn data_url_audio(request: &[u8]) -> AudioInput {
        let body = request_json(request);
        let messages = body["messages"].as_array().unwrap();
        let user = messages
            .iter()
            .find(|message| message["role"] == "user")
            .unwrap();
        let parts = user["content"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["type"], "input_audio");
        assert!(parts[0]["input_audio"].get("format").is_none());
        let data = parts[0]["input_audio"]["data"]
            .as_str()
            .unwrap()
            .strip_prefix("data:audio/wav;base64,")
            .unwrap();
        decode_wav_pcm16(&BASE64_STANDARD.decode(data).unwrap()).unwrap()
    }

    fn standard_chat_audio(request: &[u8]) -> AudioInput {
        let body = request_json(request);
        let parts = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[1]["type"], "input_audio");
        assert_eq!(parts[1]["input_audio"]["format"], "wav");
        let data = parts[1]["input_audio"]["data"].as_str().unwrap();
        decode_wav_pcm16(&BASE64_STANDARD.decode(data).unwrap()).unwrap()
    }

    #[test]
    fn automatic_data_url_chat_preserves_audio_without_inventing_text_instructions() {
        let (base_url, server) = serve_script(vec![
            ("404 Not Found", "{}"),
            (
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"heard\"}}]}",
            ),
        ]);
        let result = auto_provider(base_url).transcribe(fixture_audio());
        let requests = server.join().unwrap();
        assert_eq!(result.unwrap().text, "heard");
        assert_eq!(requests.len(), 2);
        let body = request_json(&requests[1]);
        assert_eq!(
            body["messages"].as_array().unwrap().len(),
            1,
            "no language hint means no system message"
        );
        assert_eq!(data_url_audio(&requests[1]), fixture_audio());
        for request in requests {
            assert!(String::from_utf8_lossy(&request).contains("authorization: Bearer fixture-key"));
        }
    }

    #[test]
    fn automatic_data_url_shape_rejection_negotiates_standard_chat_only_for_400_or_422() {
        for status in ["400 Bad Request", "422 Unprocessable Entity"] {
            let (base_url, server) = serve_script(vec![
                ("405 Method Not Allowed", "{}"),
                (status, "{\"error\":\"unsupported audio shape\"}"),
                (
                    "200 OK",
                    "{\"choices\":[{\"message\":{\"content\":\"heard\"}}]}",
                ),
            ]);
            let result = auto_provider(base_url).transcribe(fixture_audio());
            let requests = server.join().unwrap();
            assert_eq!(result.unwrap().text, "heard");
            assert_eq!(requests.len(), 3);
            assert_eq!(data_url_audio(&requests[1]), fixture_audio());
            assert_eq!(standard_chat_audio(&requests[2]), fixture_audio());
            assert_eq!(
                request_json(&requests[2])["messages"][0]["content"][0]["text"],
                "Please transcribe this audio exactly. Return only the transcript text."
            );
        }
    }

    #[test]
    fn automatic_chunk_negotiation_reuses_successful_formats_and_never_replays_completed_audio() {
        let (base_url, server) = serve_script(vec![
            ("404 Not Found", "{}"),
            (
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"first\"}}]}",
            ),
            ("422 Unprocessable Entity", "{}"),
            (
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"second\"}}]}",
            ),
            (
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"third\"}}]}",
            ),
            (
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"next recording\"}}]}",
            ),
        ]);
        let provider = auto_provider(base_url);
        let mut first = fixture_audio();
        first.sample_rate_hz = 1;
        first.bytes = [10, 0].repeat(60);
        let mut second = first.clone();
        second.bytes = [20, 0].repeat(60);
        let mut third = first.clone();
        third.bytes = vec![30, 0];
        let mut entire_recording = first.clone();
        entire_recording.bytes.extend_from_slice(&second.bytes);
        entire_recording.bytes.extend_from_slice(&third.bytes);
        let result = provider.transcribe(entire_recording);
        let next = provider.clone().transcribe(fixture_audio());
        let requests = server.join().unwrap();
        assert_eq!(result.unwrap().text, "first\nsecond\nthird");
        assert_eq!(next.unwrap().text, "next recording");
        assert_eq!(requests.len(), 6);
        assert!(String::from_utf8_lossy(&requests[0]).starts_with("POST /v1/audio/transcriptions "));
        assert_eq!(data_url_audio(&requests[1]), first);
        assert_eq!(data_url_audio(&requests[2]), second);
        assert_eq!(standard_chat_audio(&requests[3]), second);
        assert_eq!(standard_chat_audio(&requests[4]), third);
        assert_eq!(standard_chat_audio(&requests[5]), fixture_audio());
    }

    #[test]
    fn automatic_negotiation_never_retries_authentication_server_or_transport_errors() {
        for chat_stage in [false, true] {
            for status in [
                "401 Unauthorized",
                "403 Forbidden",
                "429 Too Many Requests",
                "500 Internal Server Error",
                "503 Service Unavailable",
                "",
            ] {
                let mut responses = Vec::new();
                if chat_stage {
                    responses.push(("404 Not Found", "{}"));
                }
                responses.push((status, "{\"error\":\"request rejected\"}"));
                let (base_url, server) = serve_script(responses);
                let result = auto_provider(base_url).transcribe(fixture_audio());
                let requests = server.join().unwrap();
                assert!(result.is_err());
                assert_eq!(
                    requests.len(),
                    if chat_stage { 2 } else { 1 },
                    "this failure must not negotiate another protocol: {status}"
                );
            }
        }
        for status in ["400 Bad Request", "422 Unprocessable Entity"] {
            let (base_url, server) = serve_script(vec![(status, "{}")]);
            let result = auto_provider(base_url).transcribe(fixture_audio());
            let requests = server.join().unwrap();
            assert!(result.is_err());
            assert_eq!(
                requests.len(),
                1,
                "multipart shape errors do not establish a missing endpoint"
            );
        }
    }

    #[test]
    fn automatic_provider_clones_share_a_thread_safe_successful_format_cache() {
        let (base_url, server) = serve_script(vec![
            ("404 Not Found", "{}"),
            (
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"heard\"}}]}",
            ),
            (
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"heard\"}}]}",
            ),
            (
                "200 OK",
                "{\"choices\":[{\"message\":{\"content\":\"heard\"}}]}",
            ),
        ]);
        let provider = auto_provider(base_url);
        let workers: Vec<_> = (0..3)
            .map(|_| {
                let provider = provider.clone();
                thread::spawn(move || provider.transcribe(fixture_audio()))
            })
            .collect();
        for worker in workers {
            assert_eq!(worker.join().unwrap().unwrap().text, "heard");
        }
        let requests = server.join().unwrap();
        assert_eq!(
            requests.len(),
            4,
            "only one caller probes multipart before the shared chat choice succeeds"
        );
        for request in &requests[1..] {
            assert_eq!(data_url_audio(request), fixture_audio());
        }
    }

    fn serve_response(
        status: &'static str,
        body: &'static str,
    ) -> (String, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "expected a recognition request");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept local request: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0, "request ended before its body was complete");
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            request
        });
        (base_url, server)
    }

    fn fixture_audio() -> AudioInput {
        AudioInput {
            bytes: vec![0, 128, 255, 127],
            sample_rate_hz: 16_000,
            channels: 1,
            format: AudioFormat::Pcm16,
        }
    }

    #[test]
    fn explicit_chat_audio_sends_standard_wav_content_with_fixed_instruction_language_and_auth() {
        let (base_url, server) = serve_response(
            "200 OK",
            "{\"choices\":[{\"message\":{\"content\":\" recognized speech \"}}]}",
        );
        let mut config = ChatAudioAsrConfig::new(
            format!(" {base_url}/audio/transcriptions/?tenant=test#ignored "),
            "fixture-key",
            "audio-transcriber",
        );
        config.language = Some("zh".to_string());
        config.timeout = Duration::from_secs(5);
        let provider = ChatAudioAsrProvider::new(config).unwrap();
        let result = provider.transcribe(fixture_audio());
        let request = server.join().unwrap();
        let end = request
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap();
        let headers = String::from_utf8_lossy(&request[..end]);
        assert!(headers.starts_with("POST /v1/chat/completions?tenant=test "));
        assert!(headers.contains("authorization: Bearer fixture-key"));
        let body: serde_json::Value = serde_json::from_slice(&request[end + 4..]).unwrap();
        assert_eq!(body["model"], "audio-transcriber");
        assert_eq!(body["stream"], false);
        let parts = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(
            parts[0]["text"],
            "Please transcribe this audio exactly. Return only the transcript text.\nLanguage hint: zh"
        );
        assert_eq!(parts[1]["type"], "input_audio");
        assert_eq!(parts[1]["input_audio"]["format"], "wav");
        let wav = BASE64_STANDARD
            .decode(parts[1]["input_audio"]["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(decode_wav_pcm16(&wav).unwrap(), fixture_audio());
        let transcript = result.unwrap();
        assert_eq!(transcript.text, "recognized speech");
        assert_eq!(transcript.language.as_deref(), Some("zh"));
    }

    #[test]
    fn recognition_adapters_return_errors_for_http_failures_malformed_and_empty_responses() {
        for chat_audio in [false, true] {
            let empty_response = if chat_audio {
                "{\"choices\":[{\"message\":{\"content\":\"  \"}}]}"
            } else {
                "{\"text\":\"  \"}"
            };
            for (status, body, expected) in [
                (
                    "401 Unauthorized",
                    "{\"error\":\"credential rejected\"}",
                    "HTTP 401",
                ),
                ("200 OK", "{\"unexpected\":true}", ""),
                ("200 OK", empty_response, "empty transcript"),
            ] {
                let (base_url, server) = serve_response(status, body);
                let provider: Box<dyn AsrProvider> = if chat_audio {
                    Box::new(
                        ChatAudioAsrProvider::new(ChatAudioAsrConfig::new(
                            base_url,
                            "fixture-key",
                            "audio-transcriber",
                        ))
                        .unwrap(),
                    )
                } else {
                    Box::new(
                        OpenAiCompatibleAsrProvider::new(OpenAiCompatibleAsrConfig::new(
                            base_url,
                            "fixture-key",
                            "transcriber",
                        ))
                        .unwrap(),
                    )
                };
                let result = provider.transcribe(fixture_audio());
                server.join().unwrap();
                let error = result.unwrap_err();
                assert!(matches!(&error, OrallyError::Asr(_)));
                if !expected.is_empty() {
                    assert!(error.to_string().contains(expected));
                }
            }
        }
    }

    #[test]
    fn asr_endpoints_accept_complete_api_routes_and_preserve_gateway_query() {
        for base_url in [
            "  https://api.example.com/gateway/v1/chat/completions/?tenant=a%2Fb#ignored  ",
            "https://api.example.com/gateway/v1/audio/transcriptions/?tenant=a%2Fb#ignored",
            " https://api.example.com/gateway/v1///?tenant=a%2Fb#ignored ",
        ] {
            assert_eq!(
                OpenAiCompatibleAsrConfig::new(base_url, "key", "model").endpoint(),
                "https://api.example.com/gateway/v1/audio/transcriptions?tenant=a%2Fb"
            );
            assert_eq!(
                ChatAudioAsrConfig::new(base_url, "key", "model").endpoint(),
                "https://api.example.com/gateway/v1/chat/completions?tenant=a%2Fb"
            );
        }
    }

    #[test]
    fn recognition_rejects_invalid_service_urls_during_construction() {
        for base_url in [
            "not a URL?secret=fixture-secret",
            "ftp://api.example.com/v1",
            "https://",
        ] {
            let multipart = OpenAiCompatibleAsrProvider::new(OpenAiCompatibleAsrConfig::new(
                base_url,
                "fixture-key",
                "model",
            ));
            let chat = ChatAudioAsrProvider::new(ChatAudioAsrConfig::new(
                base_url,
                "fixture-key",
                "model",
            ));
            for result in [multipart.map(|_| ()), chat.map(|_| ())] {
                let error = result.unwrap_err();
                assert!(matches!(&error, OrallyError::InvalidInput(_)));
                assert!(error.to_string().contains("valid HTTP or HTTPS URL"));
                assert!(!error.to_string().contains("fixture-secret"));
                assert!(!error.to_string().contains("fixture-key"));
            }
        }
    }

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
    fn builds_chat_completions_endpoint_from_v1_base_url() {
        let config = ChatAudioAsrConfig::new("https://api.example.com/v1", "key", "model");

        assert_eq!(
            config.endpoint(),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn keeps_explicit_chat_completions_endpoint() {
        let config = ChatAudioAsrConfig::new(
            "https://api.example.com/v1/chat/completions",
            "key",
            "model",
        );

        assert_eq!(
            config.endpoint(),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn rejects_empty_model() {
        let config = OpenAiCompatibleAsrConfig::new("https://api.example.com/v1", "key", "");

        assert!(OpenAiCompatibleAsrProvider::new(config).is_err());
    }

    #[test]
    fn pcm16_audio_is_split_before_transcription() {
        let seconds = MAX_TRANSCRIPTION_CHUNK_DURATION.as_secs() + 1;
        let sample_rate_hz = 16_000_u32;
        let sample_count = seconds * u64::from(sample_rate_hz);
        let audio = AudioInput {
            bytes: vec![0; sample_count as usize * 2],
            sample_rate_hz,
            channels: 1,
            format: AudioFormat::Pcm16,
        };

        let chunks = audio_to_wav_chunks(audio).expect("audio should split into WAV chunks");

        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].len() > chunks[1].len());
    }

    #[test]
    fn combines_chunk_transcripts_in_order() {
        let combined = combine_transcripts(
            vec![
                Transcript {
                    text: " first ".to_string(),
                    language: Some("en".to_string()),
                    segments: Vec::new(),
                },
                Transcript {
                    text: "second".to_string(),
                    language: None,
                    segments: Vec::new(),
                },
            ],
            None,
        )
        .expect("transcripts should combine");

        assert_eq!(combined.text, "first\nsecond");
        assert_eq!(combined.language, Some("en".to_string()));
    }
}
