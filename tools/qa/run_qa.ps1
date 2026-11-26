<#
.SYNOPSIS  Runner único dos testes de usuário do Orion (Windows, janela visível).
.EXAMPLE   .\run_qa.ps1 -Area chat,janela -Modo relatorio -Docx C:\chaves.docx
.OUTPUTS   Caminho do JSON de resultado (última linha). Códigos: 0 ok, 1 há falhas, 2 uso/ambiente.
#>
[CmdletBinding()]
param(
    [string]$Area = 'todas',
    [ValidateSet('relatorio', 'corrigir')][string]$Modo = 'relatorio',
    [string]$Docx,
    [string]$Setup,
    [string]$Bin,
    [switch]$Limpar
)
$ErrorActionPreference = 'Stop'
$qa = $PSScriptRoot
$venv = Join-Path $qa '.venv'
if (-not (Test-Path "$venv\Scripts\python.exe")) {
    python -m venv $venv
    & "$venv\Scripts\python.exe" -m pip install -q -r "$qa\requirements.txt"
}
$pyArgs = @("$qa\qa_run.py", '--area', $Area, '--modo', $Modo)
if ($Docx)  { $pyArgs += @('--docx', $Docx) }
if ($Setup) { $pyArgs += @('--setup', $Setup) }
if ($Bin)   { $pyArgs += @('--bin', $Bin) }
if ($Limpar) { $pyArgs += '--limpar' }
& "$venv\Scripts\python.exe" @args
exit $LASTEXITCODE
