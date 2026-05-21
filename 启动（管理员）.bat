@echo off
chcp 65001 >nul
powershell -NoProfile -Command "Start-Process -Verb RunAs -FilePath '%~dp0port-manager-app\src-tauri\target\release\port-manager-app.exe'"
