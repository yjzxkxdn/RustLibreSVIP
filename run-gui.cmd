@echo off
rem Launch the GUI from a console instead of the Explorer shell.
rem Console launches use CreateProcess directly, so the Windows "Open File -
rem Security Warning" (shown when the shell starts an executable it cannot
rem verify) does not appear, and the GUI inherits this console's token instead
rem of whatever the shell or a security product hands out.
setlocal
set "EXE=%~dp0target\release\libresvip-gui.exe"
if not exist "%EXE%" (
    echo %EXE% not found. Build it first:
    echo     cargo build --release -p libresvip-gui
    pause
    exit /b 1
)
start "" "%EXE%" %*
