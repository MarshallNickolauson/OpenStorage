# OpenStorage installer for Windows (PowerShell).
#   irm https://openstorage.mdnlabs.dev/install.ps1 | iex
$ErrorActionPreference = "Stop"
$dir = Join-Path $env:LOCALAPPDATA "Programs\OpenStorage"
New-Item -ItemType Directory -Force $dir | Out-Null
$exe = Join-Path $dir "OpenStorage.exe"
Write-Host "Downloading OpenStorage..."
Invoke-WebRequest "https://github.com/MarshallNickolauson/OpenStorage/releases/latest/download/openstorage-windows-x64.exe" -OutFile $exe
$lnk = Join-Path ([Environment]::GetFolderPath("Programs")) "OpenStorage.lnk"
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk); $s.TargetPath = $exe; $s.Save()
Write-Host "Installed to $exe (Start menu shortcut added)"
Start-Process $exe
