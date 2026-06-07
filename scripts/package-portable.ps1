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
    $cargoArgs = @("build", "-p", "orally-desktop", "--release")
    if ($Offline) {
      $cargoArgs += "--offline"
    }
    cargo @cargoArgs
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
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
model = "qwen3-asr-flash"
protocol = "chat-audio"
api_key_env = "DASHSCOPE_API_KEY"

[postprocess]
mode = "builtin"
base_url = "https://api.openai.com/v1"
model = ""
api_key_env = "ORALLY_LLM_API_KEY"
system_prompt = "You are Orally's dictation postprocessor. Clean speech-to-text output while preserving the user's meaning. Return only the final text, with no explanations, markdown, quotes, or labels."
user_template = """
Locale: {{locale}}
Transcript:
{{transcript}}

Rewrite the transcript into polished text suitable for direct insertion.
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
