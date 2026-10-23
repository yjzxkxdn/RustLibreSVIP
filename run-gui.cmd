@echo off
rem Launch the GUI from a console instead of the Explorer shell.
rem Console launches use CreateProcess directly, so the Windows "Open File -
rem Security Warning" (shown when the shell starts an executable it cannot
rem verify) does not appear, and the GUI inherits this console's token instead
rem of whatever the shell or a security product hands out.
rem
rem The executable is also unblocked first: a build that was copied or extracted
rem from an archive can carry the "downloaded from the Internet" mark, which is
rem what makes the shell show that warning.
setlocal
set "EXE=%~dp0target\release\libresvip-gui.exe"
if not exist "%EXE%" (
    echo %EXE% not found. Build it first:
    echo     cargo build --release -p libresvip-gui
    pause
    exit /b 1
)
powershell -NoProfile -ExecutionPolicy Bypass -Command "Unblock-File -LiteralPath '%EXE%'" >nul 2>&1
start "" "%EXE%" %*
