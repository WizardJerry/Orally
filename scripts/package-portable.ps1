param(
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$portableDir = Join-Path $root "dist\portable\Orally"
$sourceExe = Join-Path $root "target\release\orally-desktop.exe"
$targetExe = Join-Path $portableDir "Orally.exe"
$configPath = Join-Path $portableDir "config.toml"

if (-not $SkipBuild) {
  Push-Location $root
  try {
    cargo build -p orally-desktop --release
  } finally {
    Pop-Location
  }
}

if (-not (Test-Path $sourceExe)) {
  throw "Missing release executable: $sourceExe"
}

New-Item -ItemType Directory -Force -Path $portableDir | Out-Null
Copy-Item -LiteralPath $sourceExe -Destination $targetExe -Force

if (-not (Test-Path $configPath)) {
  @'
[asr]
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
model = "qwen3-asr-flash"
protocol = "chat-audio"
api_key_env = "DASHSCOPE_API_KEY"

[output]
locale = "zh-CN"
raw = false
show_changes = false
insert = true
paste_delay_ms = 300

[audio]
dictate_seconds = 3
record_output = "orally-recording.wav"

[hotkey]
preset = "ctrl-alt-space"
'@ | Set-Content -Encoding UTF8 -Path $configPath
}

Write-Host "Portable package:"
Write-Host "  $portableDir"
Write-Host "Run:"
Write-Host "  $targetExe"
