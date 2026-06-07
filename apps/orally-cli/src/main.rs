use orally_asr::{
    ChatAudioAsrConfig, ChatAudioAsrProvider, OpenAiCompatibleAsrConfig,
    OpenAiCompatibleAsrProvider,
};
use orally_audio::{
    encode_wav_pcm16, CpalAudioRecorder, CpalRecordingSession, RecordedAudio, RecordingConfig,
};
use orally_config::{AppConfig, ProviderPreset};
use orally_core::{
    AppContext, AsrProvider, AudioInput, BuiltInTextProcessor, DemoTextAsrProvider, DictionaryTerm,
    InsertMode, MemoryInserter, OrallyPipeline, PostprocessPrompt, ProcessInput, TextInserter,
    TextProcessor, Transcript,
};
use orally_windows::{run_hotkey_loop, Hotkey, WindowsClipboardPasteInserter, WindowsPasteConfig};
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();

    match args.first().map(String::as_str) {
        Some("demo") => run_demo("嗯 今天 我想 写 一封 邮件 给 visual studio code 团队"),
        Some("process") => {
            let text = args.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
            if text.trim().is_empty() {
                print_help();
                return;
            }
            run_demo(&text);
        }
        Some("record") => run_record(&args[1..]),
        Some("transcribe") => run_transcribe(&args[1..]),
        Some("dictate") => run_dictate(&args[1..]),
        Some("listen") => run_listen(&args[1..]),
        Some("config") => run_config(&args[1..]),
        _ => print_help(),
    }
}

fn load_config_or_exit() -> AppConfig {
    match orally_config::load_or_default() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Config failed: {error}");
            std::process::exit(1);
        }
    }
}

fn run_record(args: &[String]) {
    let options = match RecordOptions::parse(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            print_help();
            std::process::exit(2);
        }
    };

    println!(
        "Recording for {} second(s). Speak into your default microphone...",
        options.seconds
    );

    let recorder = CpalAudioRecorder;
    let recorded = match recorder.record_for(RecordingConfig::for_seconds(options.seconds)) {
        Ok(recorded) => recorded,
        Err(error) => {
            eprintln!("Recording failed: {error}");
            std::process::exit(1);
        }
    };

    let wav = match encode_wav_pcm16(&recorded.audio) {
        Ok(wav) => wav,
        Err(error) => {
            eprintln!("WAV encoding failed: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = fs::write(&options.output, wav) {
        eprintln!("Could not write {}: {error}", options.output.display());
        std::process::exit(1);
    }

    println!("Saved: {}", options.output.display());
    println!("Sample rate: {} Hz", recorded.audio.sample_rate_hz);
    println!("Channels: {}", recorded.audio.channels);
    println!("Duration: {} ms", recorded.metrics.duration_ms);
    println!("Peak amplitude: {:.3}", recorded.metrics.peak_amplitude);
    println!("RMS amplitude: {:.3}", recorded.metrics.rms_amplitude);
    println!(
        "Voice activity: {:.1}%",
        recorded.metrics.voice_activity_ratio * 100.0
    );
}

fn run_transcribe(args: &[String]) {
    let config = load_config_or_exit();
    let options = match TranscribeOptions::parse(args, &config) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            print_help();
            std::process::exit(2);
        }
    };

    let api = match build_asr_provider(&options.asr) {
        Ok(api) => api,
        Err(error) => {
            eprintln!("ASR configuration failed: {error}");
            std::process::exit(1);
        }
    };

    let bytes = match fs::read(&options.file) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("Could not read {}: {error}", options.file.display());
            std::process::exit(1);
        }
    };

    match api.transcribe(AudioInput::wav(bytes)) {
        Ok(transcript) => print_transcript(transcript, &options.output),
        Err(error) => {
            eprintln!("Transcription failed: {error}");
            std::process::exit(1);
        }
    }
}

fn run_dictate(args: &[String]) {
    let config = load_config_or_exit();
    let options = match DictateOptions::parse(args, &config) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            print_help();
            std::process::exit(2);
        }
    };

    let api = match build_asr_provider(&options.asr) {
        Ok(api) => api,
        Err(error) => {
            eprintln!("ASR configuration failed: {error}");
            std::process::exit(1);
        }
    };

    match dictate_once(api.as_ref(), options.seconds) {
        Ok(transcript) => print_transcript(transcript, &options.output),
        Err(error) => {
            eprintln!("Dictation failed: {error}");
            std::process::exit(1);
        }
    }
}

fn run_listen(args: &[String]) {
    let config = load_config_or_exit();
    let mut options = match ListenOptions::parse(args, &config) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            print_help();
            std::process::exit(2);
        }
    };
    options.dictate.output.insert = true;

    let api = match build_asr_provider(&options.dictate.asr) {
        Ok(api) => api,
        Err(error) => {
            eprintln!("ASR configuration failed: {error}");
            std::process::exit(1);
        }
    };

    println!("Listening for Ctrl+Alt+Space. Press Ctrl+C to stop.");
    println!("Press once to start recording, press the same hotkey again to stop and paste.");
    let mut active_recording: Option<CpalRecordingSession> = None;

    if let Err(error) = run_hotkey_loop(Hotkey::ctrl_alt_space(), |_| {
        if let Some(session) = active_recording.take() {
            eprintln!("Recording stopped. Sending audio to ASR provider...");
            let recorded = finish_recording(session)?;
            let transcript = api.transcribe(recorded.audio)?;
            print_transcript(transcript, &options.dictate.output);
        } else {
            eprintln!("Recording started. Press Ctrl+Alt+Space again to stop.");
            active_recording = Some(CpalRecordingSession::start()?);
        }
        Ok(())
    }) {
        eprintln!("Hotkey listener failed: {error}");
        std::process::exit(1);
    }
}

fn run_config(args: &[String]) {
    match args.first().map(String::as_str) {
        Some("path") => match orally_config::config_path() {
            Ok(path) => println!("{}", path.display()),
            Err(error) => {
                eprintln!("Config path failed: {error}");
                std::process::exit(1);
            }
        },
        Some("show") => match orally_config::load_or_default()
            .and_then(|config| orally_config::to_toml(&config))
        {
            Ok(text) => print!("{text}"),
            Err(error) => {
                eprintln!("Config show failed: {error}");
                std::process::exit(1);
            }
        },
        Some("init") => run_config_init(&args[1..]),
        Some("set") => run_config_set(&args[1..]),
        _ => {
            eprintln!("Usage: config path | config show | config init [--provider dashscope|openrouter|openai] [--portable] [--force] | config set <key> <value>");
            std::process::exit(2);
        }
    }
}

fn run_config_set(args: &[String]) {
    if args.len() != 2 {
        eprintln!("Usage: config set <key> <value>");
        std::process::exit(2);
    }

    let path = orally_config::config_path().unwrap_or_else(|error| {
        eprintln!("Config path failed: {error}");
        std::process::exit(1);
    });
    let mut config = orally_config::load_or_default().unwrap_or_else(|error| {
        eprintln!("Config load failed: {error}");
        std::process::exit(1);
    });

    orally_config::set_value(&mut config, &args[0], &args[1]).unwrap_or_else(|error| {
        eprintln!("Config set failed: {error}");
        std::process::exit(2);
    });
    orally_config::save_to_path(&path, &config).unwrap_or_else(|error| {
        eprintln!("Config save failed: {error}");
        std::process::exit(1);
    });

    println!("Updated {} in {}", args[0], path.display());
}

fn run_config_init(args: &[String]) {
    let mut preset = ProviderPreset::DashScope;
    let mut force = false;
    let mut portable = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--provider" | "-p" => {
                index += 1;
                let value = args.get(index).unwrap_or_else(|| {
                    eprintln!("--provider requires a value");
                    std::process::exit(2);
                });
                preset = ProviderPreset::parse(value).unwrap_or_else(|error| {
                    eprintln!("{error}");
                    std::process::exit(2);
                });
            }
            "--force" | "-f" => force = true,
            "--portable" => portable = true,
            other => {
                eprintln!("unknown config init option: {other}");
                std::process::exit(2);
            }
        }

        index += 1;
    }

    let config = preset.config();
    let result = if portable {
        orally_config::init_portable_config(&config, force)
    } else {
        orally_config::init_config(&config, force)
    };
    match result {
        Ok(path) => {
            println!("Wrote {}", path.display());
            println!("Set API key in PowerShell:");
            println!("  $env:{}=\"...\"", config.asr.api_key_env);
        }
        Err(error) => {
            eprintln!("Config init failed: {error}");
            std::process::exit(1);
        }
    }
}

fn dictate_once(
    api: &dyn AsrProvider,
    seconds: u64,
) -> Result<Transcript, orally_core::OrallyError> {
    println!("Recording for {seconds} second(s), then sending audio to ASR provider...");

    let recorder = CpalAudioRecorder;
    let recorded = recorder.record_for(RecordingConfig::for_seconds(seconds))?;
    print_recording_metrics(&recorded);

    api.transcribe(recorded.audio)
}

fn finish_recording(
    session: CpalRecordingSession,
) -> Result<RecordedAudio, orally_core::OrallyError> {
    let recorded = session.stop()?;
    print_recording_metrics(&recorded);
    Ok(recorded)
}

fn print_recording_metrics(recorded: &RecordedAudio) {
    println!(
        "Captured {} ms, peak {:.3}, voice activity {:.1}%",
        recorded.metrics.duration_ms,
        recorded.metrics.peak_amplitude,
        recorded.metrics.voice_activity_ratio * 100.0
    );
}

fn print_transcript(transcript: Transcript, options: &OutputOptions) {
    let (text, changes) = if options.raw {
        (transcript.text, Vec::new())
    } else {
        let processor = BuiltInTextProcessor;
        let processed = match processor.process(ProcessInput {
            transcript,
            context: AppContext {
                locale: options.locale.clone(),
                ..AppContext::default()
            },
            prompt: PostprocessPrompt::default(),
            dictionary_terms: default_dictionary(),
        }) {
            Ok(processed) => processed,
            Err(error) => {
                eprintln!("Post-processing failed: {error}");
                std::process::exit(1);
            }
        };
        (processed.text, processed.changes)
    };

    if options.insert {
        eprintln!(
            "Pasting in {} ms. Focus the target input field now...",
            options.paste_delay_ms
        );
        let inserter = WindowsClipboardPasteInserter::new(WindowsPasteConfig {
            paste_delay: std::time::Duration::from_millis(options.paste_delay_ms),
        });
        if let Err(error) = inserter.insert(&text, InsertMode::ClipboardFallback) {
            eprintln!("Insertion failed: {error}");
            std::process::exit(1);
        }
    } else {
        println!("{text}");
    }

    if options.show_changes && !changes.is_empty() {
        println!("Changes:");
        for change in changes {
            println!("- {change}");
        }
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

fn build_asr_provider(options: &AsrOptions) -> Result<Box<dyn AsrProvider>, String> {
    let api_key = options
        .api_key
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env::var(&options.api_key_env).ok())
        .ok_or_else(|| {
            format!(
                "missing API key: set asr.api_key or env var {}",
                options.api_key_env
            )
        })?;
    match options.resolved_protocol() {
        AsrProtocol::OpenAiTranscriptions => {
            let mut config = OpenAiCompatibleAsrConfig::new(
                options.base_url.clone(),
                api_key,
                options.model.clone(),
            );
            config.language = options.language.clone();
            config.prompt = options.prompt.clone();

            OpenAiCompatibleAsrProvider::new(config)
                .map(|provider| Box::new(provider) as Box<dyn AsrProvider>)
                .map_err(|error| error.to_string())
        }
        AsrProtocol::ChatAudio => {
            let mut config =
                ChatAudioAsrConfig::new(options.base_url.clone(), api_key, options.model.clone());
            config.language = options.language.clone();
            config.prompt = options.prompt.clone();

            ChatAudioAsrProvider::new(config)
                .map(|provider| Box::new(provider) as Box<dyn AsrProvider>)
                .map_err(|error| error.to_string())
        }
        AsrProtocol::Auto => unreachable!("auto protocol should resolve before provider build"),
    }
}

fn run_demo(text: &str) {
    let pipeline = OrallyPipeline::new(
        DemoTextAsrProvider,
        BuiltInTextProcessor,
        MemoryInserter::default(),
    );

    match pipeline.run(
        AudioInput::demo_text(text),
        AppContext::default(),
        PostprocessPrompt::default(),
        default_dictionary(),
        InsertMode::PreviewOnly,
    ) {
        Ok(processed) => {
            println!("Input:  {text}");
            println!("Output: {}", processed.text);
            if !processed.changes.is_empty() {
                println!("Changes:");
                for change in processed.changes {
                    println!("- {change}");
                }
            }
        }
        Err(error) => {
            eprintln!("Orally prototype failed: {error}");
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!("Orally CLI prototype");
    println!();
    println!("Usage:");
    println!("  cargo run -p orally-cli -- demo");
    println!("  cargo run -p orally-cli -- process \"嗯 今天 给 visual studio code 团队 写邮件\"");
    println!("  cargo run -p orally-cli -- record --seconds 3 --output orally-recording.wav");
    println!("  cargo run -p orally-cli -- transcribe --file orally-recording.wav --model <model>");
    println!("  cargo run -p orally-cli -- dictate --seconds 3 --model <model>");
    println!("  cargo run -p orally-cli -- transcribe --file orally-recording.wav --raw");
    println!("  cargo run -p orally-cli -- dictate --seconds 3 --show-changes");
    println!("  cargo run -p orally-cli -- dictate --seconds 3 --insert --paste-delay-ms 1200");
    println!("  cargo run -p orally-cli -- listen --paste-delay-ms 300");
    println!("  cargo run -p orally-cli -- config init --provider dashscope");
    println!("  cargo run -p orally-cli -- config init --provider dashscope --portable");
    println!("  cargo run -p orally-cli -- config set output.paste_delay_ms 300");
    println!("  cargo run -p orally-cli -- config show");
    println!();
    println!("ASR environment:");
    println!("  ORALLY_ASR_API_KEY       API key read by default");
    println!("  ORALLY_ASR_BASE_URL      Optional, default https://api.openai.com/v1");
    println!("  ORALLY_ASR_MODEL         Optional fallback for --model");
    println!("  ORALLY_ASR_PROTOCOL      auto, openai-transcriptions, or chat-audio");
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OutputOptions {
    raw: bool,
    show_changes: bool,
    insert: bool,
    paste_delay_ms: u64,
    locale: String,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            raw: false,
            show_changes: false,
            insert: false,
            paste_delay_ms: 750,
            locale: "zh-CN".to_string(),
        }
    }
}

impl OutputOptions {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            raw: config.output.raw,
            show_changes: config.output.show_changes,
            insert: config.output.insert,
            paste_delay_ms: config.output.paste_delay_ms,
            locale: config.output.locale.clone(),
        }
    }

    fn apply_flag(&mut self, flag: &str) -> Result<(), String> {
        match flag {
            "--raw" => self.raw = true,
            "--show-changes" => self.show_changes = true,
            "--insert" => self.insert = true,
            other => return Err(format!("unknown output flag: {other}")),
        }

        Ok(())
    }

    fn apply_option(&mut self, flag: &str, value: &str) -> Result<(), String> {
        match flag {
            "--locale" => self.locale = value.to_string(),
            "--paste-delay-ms" => {
                self.paste_delay_ms = value
                    .parse::<u64>()
                    .map_err(|_| "--paste-delay-ms must be a positive integer".to_string())?;
            }
            other => return Err(format!("unknown output option: {other}")),
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AsrProtocol {
    Auto,
    OpenAiTranscriptions,
    ChatAudio,
}

impl AsrProtocol {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "openai-transcriptions" | "multipart" => Ok(Self::OpenAiTranscriptions),
            "chat-audio" | "chat-completions" => Ok(Self::ChatAudio),
            other => Err(format!(
                "unknown ASR protocol: {other}; expected auto, openai-transcriptions, or chat-audio"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AsrOptions {
    base_url: String,
    model: String,
    api_key: Option<String>,
    api_key_env: String,
    protocol: AsrProtocol,
    language: Option<String>,
    prompt: Option<String>,
}

impl AsrOptions {
    fn defaults_from_config(config: &AppConfig) -> Self {
        let mut options = Self {
            base_url: config.asr.base_url.clone(),
            model: config.asr.model.clone(),
            api_key: config.asr.api_key.clone(),
            api_key_env: config.asr.api_key_env.clone(),
            protocol: AsrProtocol::parse(&config.asr.protocol).unwrap_or(AsrProtocol::Auto),
            language: config.asr.language.clone(),
            prompt: config.asr.prompt.clone(),
        };

        if let Ok(value) = env::var("ORALLY_ASR_BASE_URL") {
            options.base_url = value;
        }
        if let Ok(value) = env::var("ORALLY_ASR_MODEL") {
            options.model = value;
        }
        if let Ok(value) = env::var("ORALLY_ASR_API_KEY") {
            options.api_key = Some(value);
        }
        if let Ok(value) = env::var("ORALLY_ASR_API_KEY_ENV") {
            options.api_key_env = value;
        }
        if let Ok(value) = env::var("ORALLY_ASR_PROTOCOL") {
            if let Ok(protocol) = AsrProtocol::parse(&value) {
                options.protocol = protocol;
            }
        }
        if let Ok(value) = env::var("ORALLY_ASR_LANGUAGE") {
            options.language = Some(value);
        }
        if let Ok(value) = env::var("ORALLY_ASR_PROMPT") {
            options.prompt = Some(value);
        }

        options
    }

    fn apply_option(&mut self, flag: &str, value: &str) -> Result<(), String> {
        match flag {
            "--base-url" => self.base_url = value.to_string(),
            "--model" => self.model = value.to_string(),
            "--api-key" => self.api_key = Some(value.to_string()),
            "--api-key-env" => self.api_key_env = value.to_string(),
            "--protocol" => self.protocol = AsrProtocol::parse(value)?,
            "--language" => self.language = Some(value.to_string()),
            "--prompt" => self.prompt = Some(value.to_string()),
            other => return Err(format!("unknown ASR option: {other}")),
        }

        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err("missing ASR model: pass --model or set ORALLY_ASR_MODEL".to_string());
        }

        if self.base_url.trim().is_empty() {
            return Err("missing ASR base URL".to_string());
        }

        if self.api_key.is_none() && self.api_key_env.trim().is_empty() {
            return Err("missing API key env var name".to_string());
        }

        Ok(())
    }

    fn resolved_protocol(&self) -> AsrProtocol {
        if self.protocol != AsrProtocol::Auto {
            return self.protocol;
        }

        let base_url = self.base_url.to_ascii_lowercase();
        let model = self.model.to_ascii_lowercase();
        if base_url.contains("openrouter.ai")
            || base_url.contains("dashscope.aliyuncs.com/compatible-mode")
            || model.contains("qwen3-asr")
        {
            AsrProtocol::ChatAudio
        } else {
            AsrProtocol::OpenAiTranscriptions
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordOptions {
    seconds: u64,
    output: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TranscribeOptions {
    file: PathBuf,
    asr: AsrOptions,
    output: OutputOptions,
}

impl TranscribeOptions {
    fn parse(args: &[String], config: &AppConfig) -> Result<Self, String> {
        let mut file = None;
        let mut asr = AsrOptions::defaults_from_config(config);
        let mut output = OutputOptions::from_config(config);
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--file" | "-f" => {
                    index += 1;
                    file = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--file requires a value".to_string())?,
                    ));
                }
                "--base-url" | "--model" | "--api-key" | "--api-key-env" | "--protocol"
                | "--language" | "--prompt" => {
                    let flag = args[index].clone();
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| format!("{flag} requires a value"))?;
                    asr.apply_option(&flag, value)?;
                }
                "--locale" | "--paste-delay-ms" => {
                    let flag = args[index].clone();
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| format!("{flag} requires a value"))?;
                    output.apply_option(&flag, value)?;
                }
                "--raw" | "--show-changes" | "--insert" => {
                    output.apply_flag(args[index].as_str())?
                }
                "--help" | "-h" => return Err("transcribe command help".to_string()),
                other => return Err(format!("unknown transcribe option: {other}")),
            }

            index += 1;
        }

        asr.validate()?;
        Ok(Self {
            file: file.ok_or_else(|| "missing --file".to_string())?,
            asr,
            output,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DictateOptions {
    seconds: u64,
    asr: AsrOptions,
    output: OutputOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ListenOptions {
    dictate: DictateOptions,
}

impl ListenOptions {
    fn parse(args: &[String], config: &AppConfig) -> Result<Self, String> {
        let dictate = DictateOptions::parse(args, config)?;
        Ok(Self { dictate })
    }
}

impl DictateOptions {
    fn parse(args: &[String], config: &AppConfig) -> Result<Self, String> {
        let mut seconds = config.audio.dictate_seconds;
        let mut asr = AsrOptions::defaults_from_config(config);
        let mut output = OutputOptions::from_config(config);
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--seconds" | "-s" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "--seconds requires a value".to_string())?;
                    seconds = value
                        .parse::<u64>()
                        .map_err(|_| "--seconds must be a positive integer".to_string())?;
                    if seconds == 0 {
                        return Err("--seconds must be greater than zero".to_string());
                    }
                }
                "--base-url" | "--model" | "--api-key" | "--api-key-env" | "--protocol"
                | "--language" | "--prompt" => {
                    let flag = args[index].clone();
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| format!("{flag} requires a value"))?;
                    asr.apply_option(&flag, value)?;
                }
                "--locale" | "--paste-delay-ms" => {
                    let flag = args[index].clone();
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| format!("{flag} requires a value"))?;
                    output.apply_option(&flag, value)?;
                }
                "--raw" | "--show-changes" | "--insert" => {
                    output.apply_flag(args[index].as_str())?
                }
                "--help" | "-h" => return Err("dictate command help".to_string()),
                other => return Err(format!("unknown dictate option: {other}")),
            }

            index += 1;
        }

        asr.validate()?;
        Ok(Self {
            seconds,
            asr,
            output,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcribe_options_parse_output_flags() {
        let args = vec![
            "--file".to_string(),
            "sample.wav".to_string(),
            "--model".to_string(),
            "qwen3-asr-flash".to_string(),
            "--raw".to_string(),
            "--show-changes".to_string(),
            "--locale".to_string(),
            "en-US".to_string(),
            "--insert".to_string(),
            "--paste-delay-ms".to_string(),
            "1200".to_string(),
        ];

        let options =
            TranscribeOptions::parse(&args, &AppConfig::default()).expect("options should parse");

        assert_eq!(options.file, PathBuf::from("sample.wav"));
        assert!(options.output.raw);
        assert!(options.output.show_changes);
        assert!(options.output.insert);
        assert_eq!(options.output.paste_delay_ms, 1200);
        assert_eq!(options.output.locale, "en-US");
    }

    #[test]
    fn dictate_options_parse_output_flags() {
        let args = vec![
            "--seconds".to_string(),
            "2".to_string(),
            "--model".to_string(),
            "qwen3-asr-flash".to_string(),
            "--show-changes".to_string(),
        ];

        let options =
            DictateOptions::parse(&args, &AppConfig::default()).expect("options should parse");

        assert_eq!(options.seconds, 2);
        assert!(options.output.show_changes);
        assert!(!options.output.raw);
    }

    #[test]
    fn listen_options_reuse_dictate_options() {
        let args = vec![
            "--model".to_string(),
            "qwen3-asr-flash".to_string(),
            "--paste-delay-ms".to_string(),
            "250".to_string(),
        ];

        let options =
            ListenOptions::parse(&args, &AppConfig::default()).expect("options should parse");

        assert_eq!(options.dictate.output.paste_delay_ms, 250);
    }
}

impl RecordOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut seconds = 3_u64;
        let mut output = PathBuf::from("orally-recording.wav");
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--seconds" | "-s" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "--seconds requires a value".to_string())?;
                    seconds = value
                        .parse::<u64>()
                        .map_err(|_| "--seconds must be a positive integer".to_string())?;
                    if seconds == 0 {
                        return Err("--seconds must be greater than zero".to_string());
                    }
                }
                "--output" | "-o" => {
                    index += 1;
                    output = PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--output requires a value".to_string())?,
                    );
                }
                "--help" | "-h" => return Err("record command help".to_string()),
                other => return Err(format!("unknown record option: {other}")),
            }

            index += 1;
        }

        Ok(Self { seconds, output })
    }
}
