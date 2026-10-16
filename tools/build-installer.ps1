# Builds dist\AgentPals-<version>.msi from a clean staging folder (target\msi-stage), so a copy
# running from dist\agent-pals does not get in the way.
# Needs the WiX .NET tool (dotnet tool install --global wix) and its Util extension
# (wix extension add --global WixToolset.Util.wixext/5.0.2), which provides the quiet-exec action.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$stage = "target\msi-stage"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $stage | Out-Null
Copy-Item target\release\agent-pals.exe, target\release\hook-relay.exe $stage
Copy-Item profiles, themes -Destination $stage -Recurse
# Personal packs (gitignored) never ship.
Remove-Item "$stage\themes\local" -Recurse -Force -ErrorAction SilentlyContinue

# Absolute: <Files> globs resolve against the .wxs folder, not the working directory.
$stage = (Resolve-Path $stage).Path

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
New-Item -ItemType Directory dist -Force | Out-Null
$out = "dist\AgentPals-$version.msi"
# -wx: a missing harvest folder is only a warning, and would ship an installer with no themes.
wix build installer\agent-pals.wxs -arch x64 -wx -ext WixToolset.Util.wixext `
    -d Version=$version -d Dist=$stage -o $out
if ($LASTEXITCODE -ne 0) { throw "wix build failed" }
Remove-Item "dist\AgentPals-$version.wixpdb" -ErrorAction SilentlyContinue
Write-Host "Done -> $out"
