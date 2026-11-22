# ==============================================================================
# run_all.ps1 - Single-entrypoint reproduction script for Fracture Benchmark (PowerShell / Windows).
#
# Exposes Three-Tier Benchmark Execution Hierarchy:
#   1. Dry Run (-Tier dry / -Tier dry-run): Cryptographic & environment sanity check (< 10s).
#   2. Sample   (-Tier sample / -Tier small): Pilot benchmark, WAL verify, figure rendering (~20s).
#   3. Full     (-Tier full / -Tier full_benchmark): Complete test suite, benchmark replay, bootstrap CI and figure replication (default).
# ==============================================================================
param (
    [string]$Tier = "full"
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Split-Path -Parent $ScriptDir
if ($RootDir -and (Test-Path "$RootDir\Cargo.toml")) {
    Set-Location $RootDir
}

$normalized = $Tier.ToLower().TrimStart("-").Replace("-", "_")
if ($normalized -in @("dry", "dry_run")) {
    $benchTier = "dry-run"
} elseif ($normalized -in @("sample", "small", "small_test", "smoke")) {
    $benchTier = "sample"
} elseif ($normalized -in @("full", "full_benchmark")) {
    $benchTier = "full"
} else {
    Write-Error "Unknown execution tier: '$Tier'. Valid options: 'dry' (or --dry-run), 'sample' (or --sample), 'full' (or --full)."
    exit 1
}

$scriptsDir = if (Test-Path "scripts") { "scripts" } else { "Scripts" }
$resDir = if (Test-Path "results") { "results" } else { "Results" }

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  Fracture: Vector Databases Under the Crucible            " -ForegroundColor Cyan
Write-Host "  Execution Mode: $benchTier                                 " -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

if ($benchTier -eq "dry-run") {
    Write-Host "[Step 1/3] Executing dry-run sanity check..." -ForegroundColor Yellow
    cargo check --quiet
    cargo run --bin fracture --quiet -- --help > $null
    Write-Host "==========================================================" -ForegroundColor Green
    Write-Host " [DRY-RUN SUCCESSFUL] Sanity check completed (< 10s)!" -ForegroundColor Green
    Write-Host "==========================================================" -ForegroundColor Green
    exit 0
}

# 1. Execute Rust Fracture Load Generator & WAL Verifier
if ($benchTier -eq "full") {
    Write-Host "[Step 1/4] Running Rust harness unit & integration tests..." -ForegroundColor Yellow
    cargo test --quiet
}

Write-Host "[Step 1/4] Executing open-loop benchmark and point-in-time WAL replay..." -ForegroundColor Yellow
cargo run --bin fracture --quiet -- run --target mock --duration-sec 2 --qps 1000 --wal-path "$resDir/traces/run.wal" --output "$resDir/traces/benchmark.json"
cargo run --bin fracture --quiet -- verify --wal-path "$resDir/traces/run.wal" --cutoff-lsn 500 --k 10
python "$scriptsDir/generate_report.py" --input "$resDir/traces/benchmark.json" --output "$resDir/figures/dual_cdf.svg"

# 2. Render vector plots
Write-Host "[Step 2/4] Generating vector figures..." -ForegroundColor Yellow
python "$scriptsDir/plot_dual_cdf.py"
python "$scriptsDir/plot_pareto.py"
python "$scriptsDir/plot_filtering_pathology.py"
python "$scriptsDir/plot_cgroups_timeseries.py"

if ($benchTier -eq "full") {
    # 3. Compute non-parametric bootstrap confidence intervals
    Write-Host "[Step 3/4] Evaluating 95% bootstrap confidence bounds..." -ForegroundColor Yellow
    python Analysis/bootstrap_ci.py
}

# 4. Final verification of generated deliverables
Write-Host "[Step 4/4] Verifying generated benchmark deliverables..." -ForegroundColor Yellow
Get-ChildItem -Path "$resDir/figures/*.svg" | Select-Object Name, Length, LastWriteTime

Write-Host "==========================================================" -ForegroundColor Green
Write-Host " [SUCCESS - Tier: $benchTier] All deliverables generated." -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green
