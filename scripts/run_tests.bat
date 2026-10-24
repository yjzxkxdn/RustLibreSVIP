@echo off
rem Runs `cargo <args>` from the workspace root with a TMP inside the repo.
rem
rem Why this wrapper exists: on some Windows setups MSVC's cl.exe fails
rem silently when a build script invokes it (the object file never appears and
rem cargo reports exit code 2 or 1181), while clang-cl builds the same crate
rem fine. This script prefers clang-cl when it can find one, keeps TMP/TEMP in
rem target\tmp (short paths, no stale ACLs), and otherwise behaves like cargo.
rem
rem Usage:
rem   scripts\run_tests.bat test --workspace
rem   scripts\run_tests.bat build --release -p libresvip-cli
setlocal EnableExtensions

set "ROOT=%~dp0.."
pushd "%ROOT%" >nul || exit /b 1

rem Respect an existing toolchain location, otherwise use the standard one.
if not defined RUSTUP_HOME if exist "%USERPROFILE%\.rustup" set "RUSTUP_HOME=%USERPROFILE%\.rustup"
if not defined CARGO_HOME  if exist "%USERPROFILE%\.cargo"  set "CARGO_HOME=%USERPROFILE%\.cargo"
if defined CARGO_HOME if exist "%CARGO_HOME%\bin" set "PATH=%CARGO_HOME%\bin;%PATH%"

rem Prefer the LLVM shipped with Visual Studio, then anything on PATH.
set "CC_x86_64_pc_windows_msvc="
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if exist "%VSWHERE%" for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -property installationPath 2^>nul`) do (
    if exist "%%i\VC\Tools\Llvm\x64\bin\clang-cl.exe" set "CC_x86_64_pc_windows_msvc=%%i\VC\Tools\Llvm\x64\bin\clang-cl.exe"
)
if not defined CC_x86_64_pc_windows_msvc (
    where clang-cl >nul 2>nul && set "CC_x86_64_pc_windows_msvc=clang-cl"
)
if defined CC_x86_64_pc_windows_msvc echo [run_tests] using %CC_x86_64_pc_windows_msvc%

if not exist "target\tmp" mkdir "target\tmp"
set "TMP=%CD%\target\tmp"
set "TEMP=%CD%\target\tmp"

cargo %*
set "EXIT=%ERRORLEVEL%"
popd >nul
endlocal & exit /b %EXIT%
