@echo off
powershell -NoProfile -Command "$c=New-Object System.Net.Sockets.TcpClient; try{$ok=$c.ConnectAsync('127.0.0.1',6333).Wait(300)}catch{$ok=$false}; $c.Close(); if($ok){exit 1}else{exit 0}"
if %ERRORLEVEL% NEQ 0 exit /b 0
set QDRANT__STORAGE__STORAGE_PATH=C:\Orion\qdrant_data
set QDRANT__SERVICE__HTTP_PORT=6333
set QDRANT__SERVICE__HOST=127.0.0.1
cd /d C:\Orion\bin
powershell -NoProfile -Command "if ((Get-Item 'C:\Orion\bin\startup\qdrant_startup.log' -ErrorAction SilentlyContinue).Length -gt 5MB) { Move-Item 'C:\Orion\bin\startup\qdrant_startup.log' 'C:\Orion\bin\startup\qdrant_startup.log.old' -Force }"
"C:\Orion\bin\qdrant.exe" >> C:\Orion\bin\startup\qdrant_startup.log 2>&1
