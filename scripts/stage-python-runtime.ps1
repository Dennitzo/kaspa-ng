param(
    [string]$RepoRoot = "",
    [string]$OutDir = "",
    [int]$MinMinor = 10,
    [int]$MaxMinor = 13
)

$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}
if (-not $OutDir) {
    $OutDir = Join-Path $RepoRoot "target\release\python"
}

$SourceRoot = Join-Path $RepoRoot "python"

function Copy-Tree {
    param([string]$Src, [string]$Dst)
    if (-not (Test-Path -LiteralPath $Src)) { return $false }
    New-Item -ItemType Directory -Force -Path $Dst | Out-Null
    Copy-Item -Path (Join-Path $Src "*") -Destination $Dst -Recurse -Force
    return $true
}

function Get-PythonVersion {
    param([string]$Exe)
    try {
        $version = (& $Exe -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')" 2>$null)
        if ($LASTEXITCODE -ne 0 -or -not $version) { return $null }
        return $version.Trim()
    } catch {
        return $null
    }
}

function Test-PythonRuntimeRoot {
    param([string]$Root)
    if (-not $Root) { return $false }

    $pyCandidates = @(
        (Join-Path $Root "python.exe"),
        (Join-Path $Root "bin\python.exe"),
        (Join-Path $Root "Scripts\python.exe"),
        (Join-Path $Root "bin\python3"),
        (Join-Path $Root "bin\python")
    )
    $pythonExe = $pyCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    if (-not $pythonExe) { return $false }

    $version = Get-PythonVersion -Exe $pythonExe
    if (-not $version) { return $false }
    $parts = $version.Split(".")
    if ($parts.Count -lt 2) { return $false }
    $major = 0
    $minor = 0
    if (-not [int]::TryParse($parts[0], [ref]$major)) { return $false }
    if (-not [int]::TryParse($parts[1], [ref]$minor)) { return $false }
    if ($major -ne 3) { return $false }
    if ($minor -lt $MinMinor -or $minor -gt $MaxMinor) { return $false }

    & $pythonExe -c "import importlib.util; missing=[m for m in ('venv','ensurepip') if importlib.util.find_spec(m) is None]; raise SystemExit(0 if not missing else 1)" *> $null
    if ($LASTEXITCODE -ne 0) { return $false }
    & $pythonExe -m venv --help *> $null
    if ($LASTEXITCODE -ne 0) { return $false }
    & $pythonExe -m ensurepip --help *> $null
    if ($LASTEXITCODE -ne 0) { return $false }

    return $true
}

if (-not (Test-PythonRuntimeRoot -Root $SourceRoot)) {
    $fetchScript = Join-Path $RepoRoot "scripts\fetch-python-runtime.ps1"
    if (Test-Path -LiteralPath $fetchScript) {
        Write-Host "[python] bundled runtime missing or unsupported at '$SourceRoot'; attempting automatic fetch"
        & $fetchScript -RepoRoot $RepoRoot -MinMinor $MinMinor -MaxMinor $MaxMinor
    }
}

if (-not (Test-PythonRuntimeRoot -Root $SourceRoot)) {
    throw "Bundled Python runtime missing or unsupported at '$SourceRoot'."
}

if ([System.StringComparer]::OrdinalIgnoreCase.Equals($SourceRoot, $OutDir)) {
    Write-Host "[python] runtime already staged at '$OutDir'"
    exit 0
}

if (Test-Path -LiteralPath $OutDir) {
    Remove-Item -LiteralPath $OutDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$ok = Copy-Tree -Src $SourceRoot -Dst $OutDir
if (-not $ok -or -not (Test-PythonRuntimeRoot -Root $OutDir)) {
    throw "Unable to stage bundled Python runtime from '$SourceRoot' to '$OutDir'."
}

Write-Host "[python] staged runtime from '$SourceRoot' to '$OutDir'"
