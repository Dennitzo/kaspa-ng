param(
    [string]$RepoRoot = "",
    [int]$MinMinor = 10,
    [int]$MaxMinor = 13,
    [switch]$Force
)

$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
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

function Get-PythonBasePrefix {
    param([string]$Exe)
    try {
        $prefix = (& $Exe -c "import sys; print(sys.base_prefix)" 2>$null)
        if ($LASTEXITCODE -ne 0 -or -not $prefix) { return $null }
        return $prefix.Trim()
    } catch {
        return $null
    }
}

function Get-PythonRealExecutable {
    param([string]$Exe)
    try {
        $path = (& $Exe -c "import os,sys; print(os.path.realpath(sys.executable))" 2>$null)
        if ($LASTEXITCODE -ne 0 -or -not $path) { return $null }
        return $path.Trim()
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

function Resolve-PythonExe {
    $candidates = @()
    if ($env:KASPA_NG_PYTHON_BIN) {
        $candidates += $env:KASPA_NG_PYTHON_BIN
    }

    if (Get-Command "python3.13" -ErrorAction SilentlyContinue) { $candidates += (Get-Command "python3.13").Source }
    if (Get-Command "python3.12" -ErrorAction SilentlyContinue) { $candidates += (Get-Command "python3.12").Source }
    if (Get-Command "python3.11" -ErrorAction SilentlyContinue) { $candidates += (Get-Command "python3.11").Source }
    if (Get-Command "python3.10" -ErrorAction SilentlyContinue) { $candidates += (Get-Command "python3.10").Source }
    if (Get-Command "python3" -ErrorAction SilentlyContinue) { $candidates += (Get-Command "python3").Source }
    if (Get-Command "python" -ErrorAction SilentlyContinue) { $candidates += (Get-Command "python").Source }

    if ($env:LOCALAPPDATA) {
        foreach ($minor in @(13, 12, 11, 10)) {
            $candidates += (Join-Path $env:LOCALAPPDATA ("Programs\Python\Python3{0}\python.exe" -f $minor))
        }
    }

    $seen = @{}
    foreach ($candidate in $candidates) {
        if (-not $candidate) { continue }
        if ($seen.ContainsKey($candidate)) { continue }
        $seen[$candidate] = $true
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) { continue }
        $version = Get-PythonVersion -Exe $candidate
        if (-not $version) { continue }
        $parts = $version.Split(".")
        if ($parts.Count -lt 2) { continue }
        $major = 0
        $minor = 0
        if (-not [int]::TryParse($parts[0], [ref]$major)) { continue }
        if (-not [int]::TryParse($parts[1], [ref]$minor)) { continue }
        if ($major -eq 3 -and $minor -ge $MinMinor -and $minor -le $MaxMinor) {
            return $candidate
        }
    }
    return $null
}

if (-not $Force -and (Test-PythonRuntimeRoot -Root $SourceRoot)) {
    Write-Host "[python] bundled runtime already available at '$SourceRoot'"
    exit 0
}

$pythonExe = Resolve-PythonExe
if (-not $pythonExe) {
    throw "No compatible host Python found for bundling (requires Python 3.$MinMinor..3.$MaxMinor)."
}

$pythonRoot = Get-PythonBasePrefix -Exe $pythonExe
if (-not $pythonRoot) {
    throw "Unable to resolve Python base_prefix from: $pythonExe"
}
$pythonRealExe = Get-PythonRealExecutable -Exe $pythonExe
if (-not $pythonRealExe) {
    throw "Unable to resolve real Python executable from: $pythonExe"
}

if (Test-Path -LiteralPath $SourceRoot) {
    Remove-Item -LiteralPath $SourceRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $SourceRoot | Out-Null

$ok = $true
$ok = (Copy-Tree -Src (Join-Path $pythonRoot "Lib") -Dst (Join-Path $SourceRoot "Lib")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $pythonRoot "libs") -Dst (Join-Path $SourceRoot "libs")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $pythonRoot "DLLs") -Dst (Join-Path $SourceRoot "DLLs")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $pythonRoot "Scripts") -Dst (Join-Path $SourceRoot "Scripts")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $pythonRoot "include") -Dst (Join-Path $SourceRoot "include")) -and $ok

Copy-Item -LiteralPath $pythonRealExe -Destination (Join-Path $SourceRoot "python.exe") -Force

foreach ($f in @("python.exe", "python3.dll", "python312.dll", "python311.dll", "python310.dll", "vcruntime140.dll", "vcruntime140_1.dll")) {
    $src = Join-Path $pythonRoot $f
    if (Test-Path -LiteralPath $src) {
        Copy-Item -LiteralPath $src -Destination (Join-Path $SourceRoot $f) -Force
    }
}

if (-not (Test-PythonRuntimeRoot -Root $SourceRoot)) {
    throw "Fetched Python runtime at '$SourceRoot' is invalid or outside supported version range."
}

Write-Host "[python] bundled runtime prepared at '$SourceRoot'"
