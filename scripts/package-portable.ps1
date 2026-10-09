param(
  [switch]$SkipBuild,
  [switch]$Offline
)

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$portableDir = Join-Path $root "dist\portable\Orally"
$sourceExe = Join-Path $root "target\release\orally-desktop.exe"
$targetExe = Join-Path $portableDir "Orally.exe"
$configPath = Join-Path $portableDir "config.toml"
$exampleConfigPath = Join-Path $portableDir "config.example.toml"

if (-not $SkipBuild) {
  Push-Location $root
  try {
    $cargoArgs = @("build", "-p", "orally-desktop", "--release", "--locked", "--features", "tauri/custom-protocol")
    if ($Offline) {
      $cargoArgs += "--offline"
    }
    cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) {
      throw "Release build failed with exit code $LASTEXITCODE. Portable package was not updated."
    }
  } finally {
    Pop-Location
  }
}

if (-not (Test-Path $sourceExe)) {
  throw "Missing release executable: $sourceExe"
}

New-Item -ItemType Directory -Force -Path $portableDir | Out-Null
Copy-Item -LiteralPath $sourceExe -Destination $targetExe -Force

$configTemplate = @'
[asr]
base_url = "https://api.openai.com/v1"
model = "whisper-1"
protocol = "auto"
api_key_env = "ORALLY_OPENAI_COMPAT_API_KEY"

[postprocess]
mode = "llm"
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
api_key_env = "ORALLY_OPENAI_COMPAT_API_KEY"
system_prompt = "You are Orally's AI postprocessor for raw speech-to-text transcripts. Produce text that is ready to paste into the user's active app. Preserve the speaker's meaning, intent, language, names, product terms, URLs, and code identifiers. Remove filler words, repeated fragments, false starts, and self-corrections unless they change the meaning. Add only punctuation and lightweight structure that are clearly implied by the transcript. Do not invent facts, explanations, headings, labels, quotes, or markdown fences. Return only the final text."
user_template = """
Locale: {{locale}}
Task: cleanup
Transcript:
{{transcript}}

Clean the transcript into polished text in the original language. Keep normal prose unless the speaker explicitly asks for a list, translation, or another format.
"""

[output]
locale = "zh-CN"
raw = false
show_changes = false
insert = true
paste_delay_ms = 300
restore_clipboard = true
restore_clipboard_delay_ms = 250

[audio]
dictate_seconds = 3
record_output = "orally-recording.wav"
input_mode = "toggle"
auto_stop_enabled = false

[hotkey]
preset = "ctrl-alt-space"

[privacy]
allow_external_requests = true
history_enabled = true
'@

$configTemplate | Set-Content -Encoding UTF8 -Path $exampleConfigPath
if (-not (Test-Path $configPath)) {
  $configTemplate | Set-Content -Encoding UTF8 -Path $configPath
}

Write-Host "Portable package:"
Write-Host "  $portableDir"
Write-Host "Run:"
Write-Host "  $targetExe"
