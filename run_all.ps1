# ==============================================================================
# Fracture Benchmark Reproduction Script (PowerShell)
# ==============================================================================
param (
    [string]$Tier = "full"
)
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptsDir = if (Test-Path "$ScriptDir\scripts") { "$ScriptDir\scripts" } else { "$ScriptDir\Scripts" }
powershell -File "$scriptsDir\run_all.ps1" -Tier $Tier
