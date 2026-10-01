@echo off
powershell -NoProfile -Command "if ((Get-Item 'C:\Orion\bin\startup\ollama_startup.log' -ErrorAction SilentlyContinue).Length -gt 5MB) { Move-Item 'C:\Orion\bin\startup\ollama_startup.log' 'C:\Orion\bin\startup\ollama_startup.log.old' -Force }"
ollama serve >> C:\Orion\bin\startup\ollama_startup.log 2>&1
