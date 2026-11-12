@echo off
rem Carrega o ambiente do VS 2022 Build Tools (linker + Windows SDK) e roda o Unison.
rem Uso: dev.cmd            -> tauri dev (modo desenvolvimento)
rem      dev.cmd build      -> executavel de release em src-tauri\target\release\unison.exe
rem      dev.cmd installer  -> instalador NSIS por usuario (baixa a ferramenta NSIS na primeira vez)
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
set PATH=%PATH%;%USERPROFILE%\.cargo\bin
cd /d "%~dp0"
if "%1"=="build" (call npx tauri build --no-bundle) else if "%1"=="installer" (call npx tauri build --bundles nsis) else (call npx tauri dev)
