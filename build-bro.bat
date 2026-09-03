@echo off
title Build Tauri App
cls

cd /d "E:\007Launcher"

echo ==========================================
echo      BUILDING BRO JUST WAIT....      
echo ==========================================
echo.

taskkill /F /IM ciadpi.exe >nul 2>&1
taskkill /F /IM "007Launcher.exe" >nul 2>&1
taskkill /F /IM "first-light-smart-launcher.exe" >nul 2>&1

call npm run tauri build

echo.
echo ==========================================
echo  BUILD XONG ROI! 
echo ==========================================

explorer "E:\007Launcher\src-tauri\target\release"

pause
