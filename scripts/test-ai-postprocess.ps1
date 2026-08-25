# Test Orally's AI postprocessor against the temporary Aliyun Bailian
# OpenAI-compatible endpoint. Keep API keys out of the repo:
#   $env:ORALLY_OPENAI_COMPAT_API_KEY = "..."
#
# This script defaults to deepseek-v4-flash-0731 for text post-processing.
# Use qwen3-asr-flash in the ASR/transcribe path, not here.
param(
    [ValidateSet("cleanup", "outline", "translate")]
    [string]$Task = "cleanup",

    [string]$Text = "um today I want to summarize three points first performance second usability third release schedule",

    [string]$To = "English",

    [string]$Model = "deepseek-v4-flash-0731",

    [string]$BaseUrl = "https://ws-xzr3kkbjij82s72f.cn-beijing.maas.aliyuncs.com/compatible-mode/v1",

    [string]$ApiKeyEnv = "ORALLY_OPENAI_COMPAT_API_KEY"
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($ApiKeyEnv))) {
    if (-not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable("ALIYUN_TEST_API_KEY"))) {
        $ApiKeyEnv = "ALIYUN_TEST_API_KEY"
    } else {
        throw "Set `$env:$ApiKeyEnv or `$env:ALIYUN_TEST_API_KEY before running this script."
    }
}

if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    $env:CARGO_TARGET_DIR = Join-Path $env:TEMP "orally-ai-postprocess-test-target"
}

$args = @(
    "run", "-q", "-p", "orally-cli", "--",
    "process",
    "--ai",
    "--task", $Task,
    "--base-url", $BaseUrl,
    "--model", $Model,
    "--api-key-env", $ApiKeyEnv
)

if ($Task -eq "translate") {
    $args += @("--to", $To)
}

$args += @("--", $Text)

cargo @args
exit $LASTEXITCODE
