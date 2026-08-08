@echo off
echo Installing Python requirements...
pip install -r requirements.txt
echo.
echo Starting 0xo Token Server...
python main.py
pause
