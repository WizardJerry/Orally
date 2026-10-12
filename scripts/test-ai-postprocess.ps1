# Test Orally's AI postprocessor against an OpenAI-compatible endpoint.
# Override -BaseUrl and -Model for your service. Keep API keys out of the repo:
#   $env:ORALLY_OPENAI_COMPAT_API_KEY = "..."
param(
    [ValidateSet("cleanup", "outline", "translate")]
    [string]$Task = "cleanup",

    [string]$Text = "um today I want to summarize three points first performance second usability third release schedule",

    [string]$To = "English",

    [string]$Model = "gpt-4o-mini",

    [string]$BaseUrl = "https://api.openai.com/v1",

    [string]$ApiKeyEnv = "ORALLY_OPENAI_COMPAT_API_KEY"
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($ApiKeyEnv))) {
    throw "Set the API key environment variable '$ApiKeyEnv' before running this script."
}

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$cargoArgs = @(
    "build", "-q", "-p", "orally-cli",
    "--manifest-path", (Join-Path $projectRoot "Cargo.toml"),
    "--message-format=json-render-diagnostics"
)
$buildOutput = @(& cargo @cargoArgs)
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
$artifact = $buildOutput |
    ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq "compiler-artifact" -and $_.target.name -eq "orally-cli" -and $_.executable } |
    Select-Object -Last 1
if (-not $artifact) {
    throw "The CLI build did not return an executable."
}

$cliArgs = @(
    "process",
    "--ai",
    "--task", $Task,
    "--base-url", $BaseUrl,
    "--model", $Model,
    "--api-key-env", $ApiKeyEnv
)

if ($Task -eq "translate") {
    $cliArgs += @("--to", $To)
}

$cliArgs += @("--", $Text)

# The CLI always reads configuration beside itself. Run a copied executable
# with a fresh, non-secret config so legacy user configuration is never
# migrated into a build or test directory.
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testDirectoryName = "orally-ai-postprocess-" + [Guid]::NewGuid().ToString("N")
$testDirectory = Join-Path $tempRoot $testDirectoryName
New-Item -ItemType Directory -Path $testDirectory | Out-Null
$exitCode = 1
try {
    $testExe = Join-Path $testDirectory ([IO.Path]::GetFileName($artifact.executable))
    Copy-Item -LiteralPath $artifact.executable -Destination $testExe
    $fixtureConfig = @'
[privacy]
allow_external_requests = true
history_enabled = false
'@
    [IO.File]::WriteAllText(
        (Join-Path $testDirectory "config.toml"),
        $fixtureConfig,
        [Text.UTF8Encoding]::new($false)
    )
    & $testExe @cliArgs
    $exitCode = $LASTEXITCODE
} finally {
    $resolvedTestDirectory = (Resolve-Path -LiteralPath $testDirectory).Path
    $expectedTestDirectory = [IO.Path]::GetFullPath((Join-Path $tempRoot $testDirectoryName))
    if ($resolvedTestDirectory -ne $expectedTestDirectory -or
        [IO.Path]::GetFileName($resolvedTestDirectory) -ne $testDirectoryName -or
        [IO.Path]::GetDirectoryName($resolvedTestDirectory) -ne $tempRoot.TrimEnd([IO.Path]::DirectorySeparatorChar)) {
        throw "Refusing to remove an unexpected test directory: $resolvedTestDirectory"
    }
    Remove-Item -LiteralPath $resolvedTestDirectory -Recurse -Force
}

exit $exitCode
