$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$documentPairs = @(
  @("README.md", "README.zh-CN.md"),
  @("CONTEXT.md", "CONTEXT.zh-CN.md"),
  @("docs/README.md", "docs/README.zh-CN.md"),
  @("docs/product/README.md", "docs/product/README.zh-CN.md"),
  @("docs/product/architecture.md", "docs/product/architecture.zh-CN.md"),
  @("docs/product/privacy.md", "docs/product/privacy.zh-CN.md"),
  @("docs/product/roadmap.md", "docs/product/roadmap.zh-CN.md"),
  @("docs/product/grilling/README.md", "docs/product/grilling/README.zh-CN.md"),
  @("docs/engineering/refactor-baseline.md", "docs/engineering/refactor-baseline.zh-CN.md"),
  @("docs/engineering/windows-ime.md", "docs/engineering/windows-ime.zh-CN.md"),
  @("docs/adr/0001-ordered-workflow-stages.md", "docs/adr/0001-ordered-workflow-stages.zh-CN.md"),
  @("docs/adr/0002-compose-modules-into-one-request.md", "docs/adr/0002-compose-modules-into-one-request.zh-CN.md"),
  @("docs/adr/0003-share-provider-connections-across-workflows.md", "docs/adr/0003-share-provider-connections-across-workflows.zh-CN.md"),
  @("docs/adr/0004-keep-portable-data-with-the-application.md", "docs/adr/0004-keep-portable-data-with-the-application.zh-CN.md"),
  @("docs/adr/0005-use-hybrid-portable-storage.md", "docs/adr/0005-use-hybrid-portable-storage.zh-CN.md"),
  @("docs/adr/0006-store-workflows-in-sqlite-and-modules-as-toml.md", "docs/adr/0006-store-workflows-in-sqlite-and-modules-as-toml.zh-CN.md"),
  @("docs/adr/0007-stream-long-recordings-through-audio-segments.md", "docs/adr/0007-stream-long-recordings-through-audio-segments.zh-CN.md"),
  @("docs/adr/0008-edit-self-contained-model-profiles.md", "docs/adr/0008-edit-self-contained-model-profiles.zh-CN.md")
)

function Resolve-RepoPath([string]$relativePath) {
  return Join-Path $repoRoot $relativePath
}

function Get-HeadingShape([string]$content) {
  return [regex]::Matches($content, "(?m)^(#{1,6})\s+") |
    ForEach-Object { $_.Groups[1].Value.Length }
}

foreach ($pair in $documentPairs) {
  $englishRelative = $pair[0]
  $chineseRelative = $pair[1]
  $englishPath = Resolve-RepoPath $englishRelative
  $chinesePath = Resolve-RepoPath $chineseRelative

  if (-not (Test-Path -LiteralPath $englishPath)) {
    throw "Missing English document: $englishRelative"
  }
  if (-not (Test-Path -LiteralPath $chinesePath)) {
    throw "Missing Simplified Chinese document: $chineseRelative"
  }

  $english = Get-Content -LiteralPath $englishPath -Raw
  $chinese = Get-Content -LiteralPath $chinesePath -Raw
  $englishName = Split-Path $englishRelative -Leaf
  $chineseName = Split-Path $chineseRelative -Leaf

  if (-not $english.Contains("[简体中文]($chineseName)")) {
    throw "Missing Chinese navigation in $englishRelative"
  }
  if (-not $chinese.Contains("[English]($englishName)")) {
    throw "Missing English navigation in $chineseRelative"
  }

  $englishShape = (Get-HeadingShape $english) -join ","
  $chineseShape = (Get-HeadingShape $chinese) -join ","
  if ($englishShape -ne $chineseShape) {
    throw "Heading structure differs: $englishRelative <-> $chineseRelative"
  }

  if ($englishRelative.StartsWith("docs/adr/")) {
    $englishStatus = [regex]::Match($english, "(?m)^status:\s*(.+)$").Groups[1].Value.Trim()
    $chineseStatus = [regex]::Match($chinese, "(?m)^status:\s*(.+)$").Groups[1].Value.Trim()
    if ($englishStatus -ne $chineseStatus) {
      throw "ADR status differs: $englishRelative <-> $chineseRelative"
    }
  }
}

$markdownFiles = @(
  Get-Item -LiteralPath (Resolve-RepoPath "README.md"), (Resolve-RepoPath "README.zh-CN.md"),
    (Resolve-RepoPath "CONTEXT.md"), (Resolve-RepoPath "CONTEXT.zh-CN.md")
)
$markdownFiles += Get-ChildItem -LiteralPath (Resolve-RepoPath "docs") -Recurse -File -Filter "*.md"
$checkedLinks = 0

foreach ($markdownFile in $markdownFiles) {
  $content = Get-Content -LiteralPath $markdownFile.FullName -Raw
  foreach ($linkMatch in [regex]::Matches($content, "\[[^\]]*\]\((?<target>[^)]+)\)")) {
    $target = $linkMatch.Groups["target"].Value.Trim().Trim("<", ">")
    if ($target -match "^(https?://|mailto:|#)") {
      continue
    }

    $pathPart = ($target -split "#", 2)[0]
    if ([string]::IsNullOrWhiteSpace($pathPart)) {
      continue
    }

    $checkedLinks += 1
    $resolvedTarget = [System.IO.Path]::GetFullPath((Join-Path $markdownFile.DirectoryName $pathPart))
    if (-not (Test-Path -LiteralPath $resolvedTarget)) {
      throw "Broken local Markdown link: $($markdownFile.FullName) -> $target"
    }
  }
}

Write-Host "Documentation checks passed: $($documentPairs.Count) language pairs, $checkedLinks local links."
