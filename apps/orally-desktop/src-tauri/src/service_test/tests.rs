use super::*;
use orally_config::{PostprocessModelConfig, PromptNodeConfig};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn read_request(stream: &mut TcpStream) -> String {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "request ended before its body was complete");
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(header_end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let body_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            if bytes.len() >= header_end + 4 + body_length {
                return String::from_utf8_lossy(&bytes).to_string();
            }
        }
    }
}

fn serve(responses: Vec<(&'static str, String)>) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "expected a service request");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept service request: {error}"),
                }
            };
            requests.push(read_request(&mut stream));
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        requests
    });
    (base_url, server)
}

fn completion(text: &str) -> String {
    format!("{{\"choices\":[{{\"message\":{{\"content\":\"{text}\"}}}}]}}")
}

fn model(id: &str, endpoint: &str, key: &str) -> PostprocessModelConfig {
    PostprocessModelConfig {
        id: id.to_string(),
        name: format!("Draft {id}"),
        base_url: endpoint.to_string(),
        model: format!("draft-{id}"),
        api_key: Some(key.to_string()),
        api_key_env: String::new(),
        system_prompt: "base instructions".to_string(),
        user_template: "{{transcript}}".to_string(),
        fallback_to_builtin: false,
        ..PostprocessModelConfig::default()
    }
}

#[test]
fn text_test_uses_unsaved_draft_models_keys_and_prompts_in_order_without_asr() {
    let (endpoint, server) = serve(vec![
        ("200 OK", completion("first output")),
        ("200 OK", completion("final output")),
    ]);
    let mut config = AppConfig::default();
    config.asr.api_key = None;
    config.asr.api_key_env.clear();
    config.postprocess.mode = "llm".to_string();
    config.output.insert = true;
    config.privacy.history_enabled = true;
    config.privacy.history_path = Some("an unusable history path\0".to_string());
    let mut first = model("first", &endpoint, "first-draft-key");
    first.prompts = vec![
        PromptNodeConfig {
            content: "first prompt".to_string(),
            ..PromptNodeConfig::default()
        },
        PromptNodeConfig {
            content: "disabled prompt".to_string(),
            enabled: false,
            ..PromptNodeConfig::default()
        },
        PromptNodeConfig {
            content: "second prompt".to_string(),
            ..PromptNodeConfig::default()
        },
    ];
    let mut disabled = model("disabled", "not a valid endpoint", "unused-key");
    disabled.enabled = false;
    config.postprocess.models = vec![first, disabled, model("last", &endpoint, "last-draft-key")];

    let result = check_config_input(&config, Some("raw input".to_string()), None, None);
    let requests = server.join().unwrap();
    let result = result.unwrap();

    assert_eq!(result.raw_transcript, "raw input");
    assert_eq!(result.final_text, "final output");
    assert!(result.warnings.is_empty());
    assert_eq!(requests.len(), 2);
    assert!(requests[0].contains("authorization: Bearer first-draft-key"));
    assert!(requests[0].contains("\"model\":\"draft-first\""));
    assert!(requests[0]
        .contains("\"content\":\"base instructions\\n\\nfirst prompt\\n\\nsecond prompt\""));
    assert!(!requests[0].contains("disabled prompt"));
    assert!(requests[0].contains("\"content\":\"raw input\""));
    assert!(requests[1].contains("authorization: Bearer last-draft-key"));
    assert!(requests[1].contains("\"model\":\"draft-last\""));
    assert!(requests[1].contains("\"content\":\"first output\""));
    assert!(!requests[1].contains("raw input"));
}

#[test]
fn connection_tests_selected_disabled_model_without_running_its_prompts() {
    let (endpoint, server) = serve(vec![("200 OK", completion("OK"))]);
    let mut config = AppConfig::default();
    config.output.raw = true;
    let mut selected = model("selected", &endpoint, "selected-draft-key");
    selected.enabled = false;
    selected.system_prompt = "do not use these custom instructions".to_string();
    config.postprocess.models = vec![model("other", "invalid", "other-key"), selected];

    let result = check_service_connection(&config, Some("selected"));
    let requests = server.join().unwrap();

    assert!(result.unwrap().message.contains("连接成功"));
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains("authorization: Bearer selected-draft-key"));
    assert!(requests[0].contains("\"model\":\"draft-selected\""));
    assert!(requests[0].contains("Return only OK"));
    assert!(!requests[0].contains("do not use these custom instructions"));
}

#[test]
fn connection_failure_does_not_fallback_or_expose_provider_echoed_credentials() {
    let (endpoint, server) = serve(vec![(
        "401 Unauthorized",
        "{\"error\":\"Bearer private-draft-key private-draft-key\"}".to_string(),
    )]);
    let mut config = AppConfig::default();
    let mut selected = model("selected", &endpoint, "private-draft-key");
    selected.fallback_to_builtin = true;
    config.postprocess.models = vec![selected];

    let error = check_service_connection(&config, Some("selected"));
    let requests = server.join().unwrap();
    let error = error.unwrap_err();

    assert_eq!(requests.len(), 1);
    assert!(error.contains("HTTP 401"));
    assert!(!error.contains("private-draft-key"));
    assert!(!error.contains("Bearer"));
}

#[test]
fn asr_connection_uses_multipart_protocol_and_accepts_parsed_empty_silence_response() {
    let (endpoint, server) = serve(vec![(
        "200 OK",
        "{\"text\":\"\",\"language\":\"zh\"}".to_string(),
    )]);
    let mut config = AppConfig::default();
    config.asr.base_url = endpoint;
    config.asr.model = "draft-asr-model".to_string();
    config.asr.protocol = "chat-audio".to_string();
    config.asr.api_key = Some("draft-asr-key".to_string());
    config.asr.language = Some("zh".to_string());

    let result = check_service_connection(&config, None);
    let requests = server.join().unwrap();

    assert!(result.unwrap().message.contains("静音"));
    assert!(requests[0].starts_with("POST /v1/audio/transcriptions "));
    assert!(requests[0].contains("authorization: Bearer draft-asr-key"));
    assert!(requests[0].contains("draft-asr-model"));
    assert!(!requests[0].contains("name=\"prompt\""));
    assert!(requests[0].contains("RIFF"));
    assert!(requests[0].contains("WAVE"));
}

#[test]
fn asr_connection_uses_standard_transcriptions_and_rejects_malformed_response() {
    let (endpoint, server) = serve(vec![(
        "200 OK",
        "{\"invalid\":\"empty transcript\"}".to_string(),
    )]);
    let mut config = AppConfig::default();
    config.asr.base_url = endpoint;
    config.asr.model = "draft-speech-model".to_string();
    config.asr.protocol = "openai-transcriptions".to_string();
    config.asr.api_key = Some("draft-audio-key".to_string());

    let result = check_service_connection(&config, None);
    let requests = server.join().unwrap();

    assert!(result.is_err());
    assert!(requests[0].starts_with("POST /v1/audio/transcriptions "));
    assert!(requests[0].contains("authorization: Bearer draft-audio-key"));
    assert!(requests[0].contains("draft-speech-model"));
    assert!(requests[0].contains("multipart/form-data"));
    assert!(requests[0].contains("name=\"file\""));
}

#[test]
fn asr_and_llm_connection_tests_share_address_and_key_but_use_their_own_endpoints() {
    let (endpoint, server) = serve(vec![
        ("200 OK", "{\"text\":\"\"}".to_string()),
        ("200 OK", completion("OK")),
    ]);
    let mut config = AppConfig::default();
    config.asr.base_url = endpoint.clone();
    config.asr.model = "speech-model".to_string();
    config.asr.api_key = Some("shared-draft-key".to_string());
    config.asr.protocol = "chat-audio".to_string();
    config.postprocess.models = vec![model("llm", &endpoint, "shared-draft-key")];

    let speech = check_service_connection(&config, None);
    let llm = check_service_connection(&config, Some("llm"));
    let requests = server.join().unwrap();

    assert!(speech.unwrap().message.contains("连接成功"));
    assert!(llm.unwrap().message.contains("连接成功"));
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("POST /v1/audio/transcriptions "));
    assert!(requests[0].contains("multipart/form-data"));
    assert!(requests[0].contains("speech-model"));
    assert!(requests[1].starts_with("POST /v1/chat/completions "));
    assert!(requests[1].contains("application/json"));
    assert!(requests[1].contains("\"model\":\"draft-llm\""));
    assert!(requests
        .iter()
        .all(|request| request.contains("authorization: Bearer shared-draft-key")));
}

#[test]
fn asr_connection_negotiates_chat_audio_with_the_unsaved_draft() {
    let (endpoint, server) = serve(vec![
        ("404 Not Found", "{}".to_string()),
        ("200 OK", completion("")),
    ]);
    let mut config = AppConfig::default();
    config.asr.base_url = format!("{endpoint}/audio/transcriptions?tenant=draft");
    config.asr.model = "draft-audio-model".to_string();
    config.asr.protocol = "openai-transcriptions".to_string();
    config.asr.api_key = Some("unsaved-speech-key".to_string());
    config.postprocess.models = vec![model("unused", "invalid", "unused-key")];

    let result = check_service_connection(&config, None)
        .expect("an unsupported transcription route should negotiate Chat Audio");
    let requests = server.join().unwrap();

    assert!(result.message.contains("静音"));
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("POST /v1/audio/transcriptions?tenant=draft "));
    assert!(requests[1].starts_with("POST /v1/chat/completions?tenant=draft "));
    assert!(requests[1].contains("authorization: Bearer unsaved-speech-key"));
    assert!(requests[1].contains("\"model\":\"draft-audio-model\""));
    assert!(requests[1].contains("\"type\":\"input_audio\""));
    assert!(requests[1].contains("data:audio/wav;base64,UklGR"));
    assert!(!requests[1].contains("\"role\":\"system\""));
    assert!(!requests[1].contains("\"type\":\"text\""));
    assert!(!requests[1].contains("unused-key"));
    assert_eq!(config.asr.protocol, "openai-transcriptions");
}

#[test]
fn asr_chat_connection_rejects_malformed_success_instead_of_accepting_silence() {
    for body in [
        "{\"choices\":[]}",
        "{\"choices\":[{\"message\":{\"content\":null}}]}",
        "{\"choices\":[{\"message\":{}}]}",
        "{}",
    ] {
        let (endpoint, server) = serve(vec![
            ("404 Not Found", "{}".to_string()),
            ("200 OK", body.to_string()),
        ]);
        let mut config = AppConfig::default();
        config.asr.base_url = endpoint;
        config.asr.model = "audio-model".to_string();
        config.asr.api_key = Some("draft-audio-key".to_string());

        let result = check_service_connection(&config, None);
        let requests = server.join().unwrap();

        assert!(result.is_err(), "malformed response must fail: {body}");
        assert_eq!(
            requests.len(),
            2,
            "malformed success must not try a different encoding"
        );
    }
}

#[test]
fn recorded_input_negotiates_chat_audio_then_runs_unsaved_llm_pipeline() {
    let (endpoint, server) = serve(vec![
        ("404 Not Found", "{}".to_string()),
        ("200 OK", completion("raw recorded speech")),
        ("200 OK", completion("final recorded text")),
    ]);
    let mut config = AppConfig::default();
    config.asr.base_url = endpoint.clone();
    config.asr.model = "unsaved-audio-model".to_string();
    config.asr.api_key = Some("draft-shared-key".to_string());
    config.postprocess.mode = "llm".to_string();
    config.postprocess.models = vec![model("pipeline", &endpoint, "draft-shared-key")];

    let result = process_recorded_input(&config, silent_wav().unwrap(), false)
        .expect("recorded input should negotiate the ASR endpoint before refinement");
    let requests = server.join().unwrap();

    assert_eq!(result.raw_transcript, "raw recorded speech");
    assert_eq!(result.final_text, "final recorded text");
    assert_eq!(requests.len(), 3);
    assert!(requests[1].contains("data:audio/wav;base64,UklGR"));
    assert!(requests[1].contains("\"model\":\"unsaved-audio-model\""));
    assert!(requests[2].contains("\"model\":\"draft-pipeline\""));
    assert!(requests[2].contains("\"content\":\"raw recorded speech\""));
    assert!(requests
        .iter()
        .all(|request| request.contains("authorization: Bearer draft-shared-key")));
}

#[test]
fn unsupported_asr_endpoint_has_actionable_error_without_exposing_credentials() {
    let (endpoint, server) = serve(vec![
        (
            "404 Not Found",
            "{\"error\":\"Bearer private-asr-key\"}".to_string(),
        ),
        (
            "404 Not Found",
            "{\"error\":\"Bearer private-asr-key\"}".to_string(),
        ),
    ]);
    let mut config = AppConfig::default();
    config.asr.base_url = endpoint;
    config.asr.model = "speech-model".to_string();
    config.asr.api_key = Some("private-asr-key".to_string());

    let error = check_service_connection(&config, None).unwrap_err();
    let requests = server.join().unwrap();

    assert_eq!(requests.len(), 2);
    assert!(error.contains("HTTP 404"));
    assert!(requests[0].starts_with("POST /v1/audio/transcriptions "));
    assert!(requests[1].starts_with("POST /v1/chat/completions "));
    assert!(error.contains("Chat Audio"));
    assert!(!error.contains("ASR 服务需要支持 POST /audio/transcriptions"));
    assert!(!error.contains("private-asr-key"));
    assert!(!error.contains("Bearer"));
}

#[test]
fn audio_input_runs_asr_and_current_refinement_but_empty_audio_transcript_still_fails() {
    let (endpoint, server) = serve(vec![
        (
            "200 OK",
            "{\"text\":\"raw audio\",\"language\":\"en\"}".to_string(),
        ),
        ("200 OK", completion("final audio")),
    ]);
    let mut config = AppConfig::default();
    config.asr.base_url = endpoint.clone();
    config.asr.model = "draft-asr".to_string();
    config.asr.protocol = "unsupported-legacy-value".to_string();
    config.asr.api_key = Some("asr-draft-key".to_string());
    config.postprocess.mode = "llm".to_string();
    config.postprocess.models = vec![model("refinement", &endpoint, "refinement-key")];

    let result = check_config_input(
        &config,
        None,
        Some(silent_wav().unwrap().bytes),
        Some("sample.WAV"),
    );
    let requests = server.join().unwrap();
    let result = result.unwrap();

    assert_eq!(result.raw_transcript, "raw audio");
    assert_eq!(result.final_text, "final audio");
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("POST /v1/audio/transcriptions "));
    assert!(requests[1].starts_with("POST /v1/chat/completions "));
    assert!(requests[1].contains("\"content\":\"raw audio\""));

    let (endpoint, server) = serve(vec![("200 OK", "{\"text\":\"\"}".to_string())]);
    config.asr.base_url = endpoint;
    let result = check_config_input(&config, None, Some(silent_wav().unwrap().bytes), None);
    server.join().unwrap();
    assert!(result.unwrap_err().contains("empty transcript"));
}

#[test]
fn input_preserves_fallback_with_explicit_node_warning_and_redacts_error_body() {
    let (endpoint, server) = serve(vec![
        (
            "503 Service Unavailable",
            "{\"error\":\"private-fallback-key\"}".to_string(),
        ),
        ("200 OK", completion("final output")),
    ]);
    let mut config = AppConfig::default();
    config.output.locale = "en-US".to_string();
    config.postprocess.mode = "llm".to_string();
    let mut first = model("first", &endpoint, "private-fallback-key");
    first.fallback_to_builtin = true;
    config.postprocess.models = vec![first, model("last", &endpoint, "last-key")];

    let result = check_config_input(&config, Some("嗯 hello world".to_string()), None, None);
    let requests = server.join().unwrap();
    let result = result.unwrap();

    assert_eq!(result.final_text, "final output");
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("后处理节点「Draft first」（first）"));
    assert!(result.warnings[0].contains("HTTP 503"));
    assert!(!result.warnings[0].contains("private-fallback-key"));
    assert!(requests[1].contains("\"content\":\"hello world.\""));
}

#[test]
fn missing_credential_never_falls_back_and_identifies_the_failed_node() {
    let mut config = AppConfig::default();
    config.postprocess.mode = "llm".to_string();
    let mut missing_key = model("first", "http://127.0.0.1:1/v1", "");
    missing_key.api_key = None;
    missing_key.fallback_to_builtin = true;
    config.postprocess.models = vec![missing_key];

    let error = check_config_input(&config, Some("text".to_string()), None, None).unwrap_err();

    assert!(error.contains("后处理节点「Draft first」（first） (draft-first)"));
    assert!(error.contains("missing postprocess API key"));
}

#[test]
fn privacy_blocks_external_requests_but_allows_local_and_raw_text_tests() {
    let mut config = AppConfig::default();
    config.privacy.allow_external_requests = false;
    config.output.locale = "en-US".to_string();

    let local =
        check_config_input(&config, Some("嗯 hello world".to_string()), None, None).unwrap();
    assert_eq!(local.final_text, "hello world.");
    assert!(check_service_connection(&config, None)
        .unwrap_err()
        .contains("隐私"));
    assert!(
        check_config_input(&config, None, Some(silent_wav().unwrap().bytes), None)
            .unwrap_err()
            .contains("隐私")
    );

    config.postprocess.mode = "llm".to_string();
    config.postprocess.models = vec![model("first", "http://127.0.0.1:1/v1", "draft-key")];
    assert!(
        check_config_input(&config, Some("text".to_string()), None, None)
            .unwrap_err()
            .contains("隐私")
    );

    config.postprocess.models[0].enabled = false;
    let empty = check_config_input(&config, Some("嗯 text".to_string()), None, None).unwrap();
    assert_eq!(empty.final_text, "嗯 text");

    config.postprocess.models[0].enabled = true;
    config.output.raw = true;
    let raw = check_config_input(&config, Some("嗯 text".to_string()), None, None).unwrap();
    assert_eq!(raw.final_text, "嗯 text");
}

#[test]
fn input_validation_rejects_invalid_wav_oversized_files_and_ambiguous_input() {
    let config = AppConfig::default();
    assert!(check_config_input(
        &config,
        None,
        Some(vec![0; MAX_AUDIO_BYTES + 1]),
        Some("large.wav")
    )
    .unwrap_err()
    .contains("15 MiB"));
    assert!(check_config_input(
        &config,
        None,
        Some(b"not a WAV".to_vec()),
        Some("audio.wav")
    )
    .unwrap_err()
    .contains("WAV"));
    assert!(
        check_config_input(&config, Some("text".to_string()), Some(Vec::new()), None)
            .unwrap_err()
            .contains("其中一种")
    );
    assert!(check_config_input(&config, None, None, None).is_err());
    assert!(check_config_input(&config, Some(" ".to_string()), None, None).is_err());
    assert!(
        check_config_input(&config, Some("文".repeat(MAX_TEXT_CHARS + 1)), None, None).is_err()
    );
}

#[test]
fn sanitized_errors_redact_direct_environment_credentials_and_urls() {
    let variable = format!("ORALLY_NATIVE_TEST_SECRET_{}", std::process::id());
    std::env::set_var(&variable, "private-env-key");
    let mut config = AppConfig::default();
    config.asr.api_key = Some("private-direct-key".to_string());
    config.postprocess.api_key_env = variable.clone();

    let error = sanitize_error("private-direct-key private-env-key", &config);
    let network = sanitize_error(
        "error sending request for url (https://provider.invalid/v1?token=other-secret)",
        &config,
    );
    std::env::remove_var(variable);

    assert_eq!(error, "[redacted] [redacted]");
    assert!(!network.contains("other-secret"));
    assert!(!network.contains("provider.invalid"));
}
