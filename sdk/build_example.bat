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

:: ── 3. Compile pingpong.cpp ───────────────────────────────────────────────────
echo [build] Setting up MSVC environment...
call "%VCVARS%" >nul 2>&1

echo [build] Compiling examples\pingpong.cpp...
cl /nologo /EHsc /std:c++17 /I. examples\pingpong.cpp target\debug\wl_sdk.dll.lib /Fe:pingpong.exe
if errorlevel 1 (
    echo [build] ERROR: Compilation failed
    exit /b 1
)

:: DLL must be findable at runtime
copy /y target\debug\wl_sdk.dll wl_sdk.dll >nul

echo.
echo [build] Done: pingpong.exe  (wl_sdk.dll copied alongside)
endlocal
