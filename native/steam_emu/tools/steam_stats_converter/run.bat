@echo off
:: Thin wrapper — forwards all arguments to steam_stats_converter.py
:: Examples:
::   run.bat --appid 1234 --st2gse
::   run.bat --appid 1234 --gse2st
::   run.bat --appid 1234 --info
::   run.bat --help
python "%~dp0steam_stats_converter.py" %*
