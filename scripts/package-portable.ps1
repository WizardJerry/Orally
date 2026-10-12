param(
  [switch]$SkipBuild,
  [switch]$Offline,
  [string]$OutputDirectory = "dist\portable\Orally"
)

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$portableDir = if ([IO.Path]::IsPathRooted($OutputDirectory)) { $OutputDirectory } else { Join-Path $root $OutputDirectory }
$sourceExe = Join-Path $root "target\release\orally-desktop.exe"
$targetExe = Join-Path $portableDir "Orally.exe"
$configPath = Join-Path $portableDir "config.toml"
$exampleConfigPath = Join-Path $portableDir "config.example.toml"
$settingsPath = Join-Path $portableDir "setting.toml"
$profilesDir = Join-Path $portableDir "profiles"

if (-not $SkipBuild) {
  Push-Location $root
  try {
    $npmArgs = @("--prefix", "apps/orally-desktop", "ci", "--include=dev", "--prefer-offline", "--no-audit", "--no-fund")
    if ($Offline) {
      $npmArgs += "--offline"
    }
    npm.cmd @npmArgs
    if ($LASTEXITCODE -ne 0) {
      throw "Frontend dependency installation failed with exit code $LASTEXITCODE. Portable package was not updated."
    }
    npm.cmd --prefix apps/orally-desktop run ui:build
    if ($LASTEXITCODE -ne 0) {
      throw "Frontend build failed with exit code $LASTEXITCODE. Portable package was not updated."
    }
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
Copy-Item -LiteralPath (Join-Path $root "apps\orally-desktop\third-party-licenses") -Destination $portableDir -Recurse -Force

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
insert = true
direct_output = true
auto_copy = false

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
$parts = $configTemplate -split '(?m)(?=^\[output\])', 2
$profileTemplate = "id = `"profile-1`"`nname = `"Default`"`n`n" + $parts[0]
$settingsTemplate = "active_profile_id = `"profile-1`"`nprofile_order = [`"profile-1`"]`n`n" + $parts[1] + "`n`n[appearance]`ndark_mode = true`n"
$profileTemplate | Set-Content -Encoding UTF8 -Path (Join-Path $portableDir "profile.example.toml")
$settingsTemplate | Set-Content -Encoding UTF8 -Path (Join-Path $portableDir "setting.example.toml")
# Existing split files or legacy config belong to the user. Runtime migration
# handles the latter on first launch; packaging never replaces either format.
if (-not (Test-Path -LiteralPath $settingsPath) -and -not (Test-Path -LiteralPath $configPath) -and -not (Test-Path -LiteralPath $profilesDir)) {
  New-Item -ItemType Directory -Path $profilesDir | Out-Null
  $profileTemplate | Set-Content -Encoding UTF8 -Path (Join-Path $profilesDir "default.toml")
  $settingsTemplate | Set-Content -Encoding UTF8 -Path $settingsPath
}

Write-Host "Portable package:"
Write-Host "  $portableDir"
Write-Host "Run:"
Write-Host "  $targetExe"
