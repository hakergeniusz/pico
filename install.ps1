#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Version,
    [string]$BaseUrl = "https://github.com/hakergeniusz/pico/releases/download",
    [string]$InstallDir = "$env:LOCALAPPDATA\pico\bin",
    [string]$Token
)

# Windows installer for pico.
# Usage: irm https://raw.githubusercontent.com/hakergeniusz/pico/feat/basic-editor/install.ps1 | iex
# Env equivalents: PICO_VERSION, PICO_BASE_URL, PICO_INSTALL_DIR, PICO_TOKEN.

$ErrorActionPreference = "Stop"
$Repo = "hakergeniusz/pico"
$DefaultBase = "https://github.com/$Repo/releases/download"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

if (-not $Version -and $env:PICO_VERSION) { $Version = $env:PICO_VERSION }
if ($BaseUrl -eq $DefaultBase -and $env:PICO_BASE_URL) { $BaseUrl = $env:PICO_BASE_URL }
if ($InstallDir -eq "$env:LOCALAPPDATA\pico\bin" -and $env:PICO_INSTALL_DIR) {
    $InstallDir = $env:PICO_INSTALL_DIR
}
if (-not $Token) { $Token = $env:PICO_TOKEN }

function Write-Info {
    param([string]$Message)
    Write-Host "pico installer: $Message"
}

function Get-Remote {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [string]$OutFile
    )
    $headers = @{}
    if ($Token) { $headers["Authorization"] = "Bearer $Token" }
    if ($OutFile) {
        Invoke-WebRequest -Uri $Url -Headers $headers -OutFile $OutFile -UseBasicParsing
        return
    }
    return [string](Invoke-WebRequest -Uri $Url -Headers $headers -UseBasicParsing).Content
}

switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { $Target = "x86_64-pc-windows-msvc" }
    "ARM64" { throw "pico does not publish a Windows ARM64 build yet" }
    default { throw "unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE" }
}

if (-not $Version) {
    $releaseJson = Get-Remote "https://api.github.com/repos/$Repo/releases/latest"
    $release = $releaseJson | ConvertFrom-Json
    $Version = $release.tag_name
}
if (-not $Version) { throw "could not determine the latest release tag" }

$VersionNumber = $Version.TrimStart("v")
$DownloadBase = if ($BaseUrl -eq $DefaultBase) { "$BaseUrl/$Version" } else { $BaseUrl }
$Archive = "pico-$VersionNumber-$Target.zip"

$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("pico-install-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $Tmp -Force | Out-Null

try {
    Write-Info "Downloading pico $VersionNumber for $Target"
    Get-Remote "$DownloadBase/$Archive" (Join-Path $Tmp $Archive)
    Get-Remote "$DownloadBase/$Archive.sha256" (Join-Path $Tmp "$Archive.sha256")

    $checksumLine = (Get-Content (Join-Path $Tmp "$Archive.sha256") -First 1)
    if ($checksumLine -notmatch '^([0-9a-fA-F]{64})') {
        throw "malformed checksum file"
    }
    $Expected = $Matches[1].ToLowerInvariant()
    $Actual = (Get-FileHash (Join-Path $Tmp $Archive) -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Actual -ne $Expected) { throw "checksum verification failed" }

    $Stage = Join-Path $Tmp "stage"
    Expand-Archive -Path (Join-Path $Tmp $Archive) -DestinationPath $Stage -Force
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item (Join-Path $Stage "pico-$VersionNumber-$Target\pico.exe") `
        (Join-Path $InstallDir "pico.exe") -Force

    Write-Info "Installed $InstallDir\pico.exe"
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        Write-Info "Add $InstallDir to your PATH to use pico"
    }
}
finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
