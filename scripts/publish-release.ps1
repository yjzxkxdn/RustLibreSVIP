# Publishes a GitHub release with the artifacts built for it.
#
#   $env:GITHUB_TOKEN = 'ghp_...'          # fine-grained token with "Contents: write"
#   .\scripts\publish-release.ps1 -Tag v1.0.0
#   .\scripts\publish-release.ps1 -Tag v1.0.0 -DryRun      # show what would be sent
#
# It creates (or reuses) the release for the tag and uploads every file in
# `dist\<version>` (or `<repo>\..\dist\<version>`) that is not a directory.
# Already uploaded assets are skipped, so re-running is safe.
#
# Note: Windows PowerShell 5.1 encodes a plain -Body string as ISO-8859-1, which
# corrupts the Chinese release notes ("Problems parsing JSON"). The body is
# therefore converted to UTF-8 bytes explicitly.

param(
    [string]$Tag = 'v1.0.0',
    [string]$Repo = 'yjzxkxdn/RustLibreSVIP',
    [string]$Assets = '',
    [string]$Notes = '',
    [string]$Token = $env:GITHUB_TOKEN,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path | Split-Path -Parent
$version = $Tag.TrimStart('v')

if (-not $Assets) {
    $candidates = @((Join-Path $root "dist\$version"), (Join-Path (Split-Path $root -Parent) "dist\$version"))
    $Assets = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $Assets) { throw "missing artifact folder, looked in: $($candidates -join ', ')" }
}
if (-not (Test-Path $Assets)) { throw "missing artifact folder: $Assets" }

if (-not $Notes) {
    $noteCandidates = @((Join-Path $root 'dist\RELEASE_NOTES.md'), (Join-Path (Split-Path $root -Parent) 'dist\RELEASE_NOTES.md'))
    $Notes = $noteCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}

# Read as UTF-8 so the Chinese text in the release notes survives.
$bodyText = if ($Notes -and (Test-Path $Notes)) { [IO.File]::ReadAllText($Notes, [Text.Encoding]::UTF8) } else { '' }
$files = @(Get-ChildItem $Assets -File | Sort-Object Name)

Write-Host "tag      : $Tag"
Write-Host "assets   : $Assets ($($files.Count) files)"
Write-Host "notes    : $Notes"

if ($DryRun) {
    $preview = @{ tag_name = $Tag; name = "RustLibreSVIP $version"; body = $bodyText; draft = $false; prerelease = $false } |
        ConvertTo-Json -Depth 3
    Write-Host "dry run, no network call. JSON head:"
    Write-Host $preview.Substring(0, [Math]::Min(120, $preview.Length))
    $files | ForEach-Object { Write-Host ("  would upload {0} ({1} MB)" -f $_.Name, [Math]::Round($_.Length / 1MB, 2)) }
    return
}

if (-not $Token) { throw 'Set GITHUB_TOKEN (Contents: write) first.' }

$headers = @{
    Authorization          = "Bearer $Token"
    Accept                 = 'application/vnd.github+json'
    'X-GitHub-Api-Version' = '2022-11-28'
    'User-Agent'           = 'RustLibreSVIP-release'
}

$body = @{ tag_name = $Tag; name = "RustLibreSVIP $version"; body = $bodyText; draft = $false; prerelease = $false }
$bodyBytes = [Text.Encoding]::UTF8.GetBytes(($body | ConvertTo-Json -Depth 3))

$release = $null
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/tags/$Tag" -Headers $headers
    Write-Host "reusing release $($release.id) ($($release.assets.Count) assets)"
} catch {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases" -Method Post -Headers $headers `
        -ContentType 'application/json; charset=utf-8' -Body $bodyBytes
    Write-Host "created release $($release.id)"
}

# `$uploadUrl?name=` would be parsed as a variable named `uploadUrl?name`, so concatenate.
$uploadUrl = ($release.upload_url -replace '\{.*$', '')
$existing = @($release.assets | ForEach-Object { $_.name })

foreach ($f in $files) {
    if ($existing -contains $f.Name) { Write-Host "skip (already uploaded): $($f.Name)"; continue }
    Write-Host "uploading $($f.Name) ($([math]::Round($f.Length / 1MB, 2)) MB)"
    $uri = $uploadUrl + '?name=' + [uri]::EscapeDataString($f.Name)
    Invoke-RestMethod -Uri $uri -Method Post -Headers $headers -ContentType 'application/octet-stream' -InFile $f.FullName | Out-Null
}

$check = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/tags/$Tag" -Headers $headers
Write-Host "release $Tag published: https://github.com/$Repo/releases/tag/$Tag"
$check.assets | Sort-Object name | ForEach-Object { Write-Host ("  {0,-30} {1,10} bytes" -f $_.name, $_.size) }
