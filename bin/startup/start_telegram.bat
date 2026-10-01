@echo off
REM Lyra Telegram Bot — polling bidirecional com o Telegram
REM Requer TELEGRAM_BOT_TOKEN no .env (obter com @BotFather)

REM Verifica se já está rodando
tasklist /FI "IMAGENAME eq python.exe" /FI "WINDOWTITLE eq Lyra_Telegram*" 2>nul | find "python.exe" >nul
if %ERRORLEVEL% EQU 0 (
    echo Bot Telegram ja esta rodando.
    exit /b 0
)

REM Checa token configurado
for /f "tokens=2 delims==" %%a in ('findstr /i "TELEGRAM_BOT_TOKEN" "C:\Orion\Orion_Ollama\.env"') do set TOKEN=%%a
if "%TOKEN%"=="" (
    echo [ERRO] TELEGRAM_BOT_TOKEN nao definido no .env
    echo Fale com @BotFather no Telegram, crie um bot e cole o token em .env
    exit /b 1
)

title Lyra_Telegram
"C:\Users\anton\AppData\Local\Programs\Python\Python312\python.exe" ^
    "C:\Orion\Orion_Ollama\orion_telegram.py" ^
    >> "C:\Orion\bin\startup\telegram_startup.log" 2>&1
