@echo off
setlocal

:: ── 1. Build the Rust SDK ─────────────────────────────────────────────────────
echo [build] Building Rust SDK...
cargo build
if errorlevel 1 (
    echo [build] ERROR: cargo build failed
    exit /b 1
)

:: ── 2. Locate MSVC via vswhere ────────────────────────────────────────────────
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSWHERE%" (
    echo [build] ERROR: vswhere not found. Is Visual Studio installed?
    exit /b 1
)

for /f "usebackq delims=" %%i in (
    `"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`
) do set "VS_PATH=%%i"

if not defined VS_PATH (
    echo [build] ERROR: No Visual Studio installation with MSVC found.
    exit /b 1
)

set "VCVARS=%VS_PATH%\VC\Auxiliary\Build\vcvars64.bat"
if not exist "%VCVARS%" (
    echo [build] ERROR: vcvars64.bat not found at: %VCVARS%
    exit /b 1
)

:: ── 3. Compile pingpong.c ─────────────────────────────────────────────────────
echo [build] Setting up MSVC environment...
call "%VCVARS%" >nul 2>&1

echo [build] Compiling examples\pingpong.c...
cl /nologo /I. examples\pingpong.c target\debug\wl_sdk.lib Ws2_32.lib Userenv.lib ntdll.lib /Fe:pingpong.exe
if errorlevel 1 (
    echo [build] ERROR: Compilation failed
    exit /b 1
)

echo.
echo [build] Done: pingpong.exe
endlocal
