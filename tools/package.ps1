# Builds release and assembles dist\agent-pals\ (exe + relay + profiles + themes + config).
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
cargo build --release
$out = "dist\agent-pals"
Remove-Item $out -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $out | Out-Null
Copy-Item target\release\agent-pals.exe, target\release\hook-relay.exe, config.json $out
Copy-Item profiles, themes -Destination $out -Recurse
Write-Host "Done -> $out"
