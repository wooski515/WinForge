@echo off
rem WinForge launcher: double-click to start.
rem Uses WinForge.exe next to this file, else the built exe, else builds it (needs Rust).
setlocal
cd /d "%~dp0"
if exist "WinForge.exe" (start "" "WinForge.exe" & exit /b 0)
if exist "target\release\winforge.exe" (start "" "target\release\winforge.exe" & exit /b 0)
where cargo >nul 2>&1
if errorlevel 1 (
  echo WinForge is not built yet, and Rust is not installed.
  echo Download the ready WinForge.exe: https://github.com/wooski515/WinForge/releases/latest
  echo Or install Rust from https://rustup.rs to build it here.
  pause
  exit /b 1
)
echo Building WinForge for the first time, this takes a few minutes...
cargo build --release
if errorlevel 1 (pause & exit /b 1)
start "" "target\release\winforge.exe"
