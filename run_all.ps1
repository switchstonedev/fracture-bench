# ==============================================================================
# ACM / IEEE Artifact Evaluation Single-Entrypoint Reproduction Script (PowerShell)
# Fracture Artifact Evaluation
# ==============================================================================
param (
    [string]$Tier = "full"
)
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptsDir = if (Test-Path "$ScriptDir\scripts") { "$ScriptDir\scripts" } else { "$ScriptDir\Scripts" }
powershell -File "$scriptsDir\run_all.ps1" -Tier $Tier
