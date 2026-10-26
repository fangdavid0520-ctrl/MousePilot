[CmdletBinding()]
param(
    [switch]$DebugBuild,
    [switch]$Run
)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargoCommand) {
    $candidate = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    if (Test-Path $candidate) {
        $cargo = $candidate
    } else {
        Write-Host "未找到 Rust / Cargo。" -ForegroundColor Red
        Write-Host "请安装 Rust（https://rustup.rs），并安装 Visual Studio C++ Build Tools。"
        exit 1
    }
} else {
    $cargo = $cargoCommand.Source
}

$arguments = @("build")
if (-not $DebugBuild) {
    $arguments += "--release"
}

Write-Host "正在构建 MousePilot..." -ForegroundColor Cyan
& $cargo @arguments
if ($LASTEXITCODE -ne 0) {
    Write-Host "构建失败，请查看上方 Cargo 输出。" -ForegroundColor Red
    exit $LASTEXITCODE
}

$profileName = if ($DebugBuild) { "debug" } else { "release" }
$source = Join-Path $PSScriptRoot "target\$profileName\mousepilot.exe"
if (-not (Test-Path $source)) {
    Write-Host "构建完成，但未找到 $source" -ForegroundColor Red
    exit 1
}

if (-not $DebugBuild) {
    $dist = Join-Path $PSScriptRoot "dist"
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    $destination = Join-Path $dist "MousePilot.exe"
    Copy-Item $source $destination -Force
    Copy-Item (Join-Path $PSScriptRoot "README.md") $dist -Force
    $docs = Join-Path $PSScriptRoot "docs"
    if (Test-Path $docs) {
        $docsDestination = Join-Path $dist "docs"
        New-Item -ItemType Directory -Force -Path $docsDestination | Out-Null
        Copy-Item (Join-Path $docs "*") $docsDestination -Recurse -Force
    }
    Write-Host "已生成: $destination" -ForegroundColor Green
} else {
    $destination = $source
    Write-Host "已生成: $destination" -ForegroundColor Green
}

if ($Run) {
    Start-Process $destination
}
