# scripts/package.ps1 [-Out DIR]: builds The Little Prince for Windows x64 and assembles the app
# folder (dist/littleprince-windows by default) and its .gzl, which Gizlet Launcher for Windows
# installs.
#
# Needs Windows, Python 3.11+, the Rust x86_64-pc-windows-msvc target with Visual Studio's C++
# tools, CMake (raylib-sys builds raylib) and LLVM (bindgen's libclang).
param([string]$Out)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
if (-not $Out) { $Out = Join-Path $root 'dist/littleprince-windows' }
$target = 'x86_64-pc-windows-msvc'
if (-not $env:LIBCLANG_PATH -and (Test-Path 'C:\Program Files\LLVM\bin\libclang.dll')) { $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin' }

$version = python -c 'import tomllib; print(tomllib.load(open("gizlet.toml", "rb"))["version"])'
if ($LASTEXITCODE -ne 0) { throw 'cannot read gizlet.toml' }
$env:GIZLET_APP_VERSION = $version
cargo build --release --locked --target $target
if ($LASTEXITCODE -ne 0) { throw "cargo build exited with $LASTEXITCODE" }

if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force "$Out/bin", "$Out/licenses", "$Out/assets/hero" | Out-Null
Copy-Item "target/$target/release/littleprince.exe" "$Out/bin/littleprince.exe"
Copy-Item LICENSE "$Out/licenses/littleprince-MIT.txt"
Copy-Item assets/hero/little-prince.glb "$Out/assets/hero/"
Copy-Item assets/hero/LICENSE.txt "$Out/licenses/little-prince-model-CC0.txt"
Copy-Item assets/fonts/Inter-OFL.txt "$Out/licenses/inter-OFL.txt"
Copy-Item assets/fonts/SpaceGrotesk-OFL.txt "$Out/licenses/space-grotesk-OFL.txt"
Copy-Item assets/fonts/DroidSansFallback-LICENSE.txt "$Out/licenses/droid-sans-fallback-Apache-2.0.txt"
python scripts/pack.py $Out $target
if ($LASTEXITCODE -ne 0) { throw "pack.py exited with $LASTEXITCODE" }
