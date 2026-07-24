$ErrorActionPreference = "Stop"

$Repo = "madLinux7/svg-strip"
$InstallDir = if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { "$env:LOCALAPPDATA\svg-strip" }
$Binary = "svg-strip.exe"

Write-Host "Fetching latest release..."
$Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
$Tag = $Release.tag_name
if (-not $Tag) {
    Write-Error "Could not determine latest release"
    exit 1
}

$Url = "https://github.com/$Repo/releases/download/$Tag/$Binary"
Write-Host "Downloading svg-strip $Tag..."

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$OutFile = Join-Path $InstallDir $Binary
Invoke-WebRequest -Uri $Url -OutFile $OutFile

Write-Host "svg-strip $Tag installed to $OutFile"

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$InstallDir;$UserPath", "User")
    Write-Host "$InstallDir added to your PATH. Restart your terminal to use svg-strip."
}
