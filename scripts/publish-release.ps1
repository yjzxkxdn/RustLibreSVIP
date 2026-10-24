# Publishes a GitHub release with the artifacts built for it.
#
#   $env:GITHUB_TOKEN = 'ghp_...'          # fine-grained token with "Contents: write"
#   .\scripts\publish-release.ps1 -Tag v1.0.0
#
# It creates (or reuses) the release for the tag and uploads every file in
# `dist\<version>` that is not a directory.

param(
    [string]$Tag = 'v1.0.0',
    [string]$Repo = 'yjzxkxdn/RustLibreSVIP',
    [string]$Assets = '',
    [string]$Notes = '',
    [string]$Token = $env:GITHUB_TOKEN
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path | Split-Path -Parent
$version = $Tag.TrimStart('v')
if (-not $Assets) { $Assets = Join-Path $root "dist\$version" }
if (-not $Notes) { $Notes = Join-Path $root "dist\RELEASE_NOTES.md" }
if (-not $Token) { throw 'Set GITHUB_TOKEN (Contents: write) first.' }
if (-not (Test-Path $Assets)) { throw "missing artifact folder: $Assets" }

$headers = @{
    Authorization           = "Bearer $Token"
    Accept                  = 'application/vnd.github+json'
    'X-GitHub-Api-Version'  = '2022-11-28'
    'User-Agent'            = 'RustLibreSVIP-release'
}
$body = @{ tag_name = $Tag; name = "RustLibreSVIP $version"; body = (Get-Content $Notes -Raw); draft = $false; prerelease = $false }

$release = $null
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/tags/$Tag" -Headers $headers
} catch {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases" -Method Post -Headers $headers -ContentType 'application/json' -Body ($body | ConvertTo-Json)
}
$uploadUrl = ($release.upload_url -replace '\{.*$', '')

Get-ChildItem $Assets -File | ForEach-Object {
    $name = $_.Name
    Write-Host "uploading $name ($([math]::Round($_.Length / 1MB, 2)) MB)"
    Invoke-RestMethod -Uri "$uploadUrl?name=$name" -Method Post -Headers $headers -ContentType 'application/octet-stream' -InFile $_.FullName | Out-Null
}
Write-Host "release $Tag published: https://github.com/$Repo/releases/tag/$Tag"
