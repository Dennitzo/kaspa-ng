param(
    [Parameter(Mandatory = $true)]
    [string]$ArtifactRoot
)

$ErrorActionPreference = "Stop"

function Require-Directory {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        throw "Missing required directory: $Path"
    }
}

function Require-File {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Missing required file: $Path"
    }
}

function Resolve-PythonExe {
    $candidates = @()
    if ($env:KASPA_NG_PYTHON_BIN) {
        $candidates += $env:KASPA_NG_PYTHON_BIN
    }

    $pythonCmd = Get-Command python -ErrorAction SilentlyContinue
    if ($pythonCmd) {
        $candidates += $pythonCmd.Source
    }

    $python3Cmd = Get-Command python3 -ErrorAction SilentlyContinue
    if ($python3Cmd) {
        $candidates += $python3Cmd.Source
    }

    if ($env:LOCALAPPDATA) {
        foreach ($minor in @(14, 13, 12, 11, 10)) {
            $candidates += (Join-Path $env:LOCALAPPDATA ("Programs\Python\Python3{0}\python.exe" -f $minor))
        }
    }

    $seen = @{}
    foreach ($candidate in $candidates) {
        if (-not $candidate) {
            continue
        }
        if ($seen.ContainsKey($candidate)) {
            continue
        }
        $seen[$candidate] = $true
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            continue
        }

        $version = (& $candidate -c "import sys; print(f'{sys.version_info[0]}.{sys.version_info[1]}')" 2>$null)
        if ($LASTEXITCODE -ne 0 -or -not $version) {
            continue
        }
        $parts = $version.Trim().Split(".")
        if ($parts.Count -lt 2) {
            continue
        }
        $major = 0
        $minor = 0
        if (-not [int]::TryParse($parts[0], [ref]$major)) {
            continue
        }
        if (-not [int]::TryParse($parts[1], [ref]$minor)) {
            continue
        }
        if ($major -eq 3 -and $minor -ge 10) {
            return $candidate
        }
    }

    return $null
}

function Test-ServerVenvModules {
    param(
        [string]$ServerName,
        [string]$ServerRoot,
        [string[]]$Modules
    )

    $venvCandidates = @(
        (Join-Path $ServerRoot ".venv\Scripts\python.exe"),
        (Join-Path $ServerRoot ".venv\bin\python3"),
        (Join-Path $ServerRoot ".venv\bin\python")
    )
    $venvPython = $venvCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1

    if (-not $venvPython) {
        Write-Host "[python-runtime] $ServerName`: no packaged .venv found (runtime bootstrap expected)"
        return
    }

    $probeScript = @'
import importlib.util
import sys
missing = [m for m in sys.argv[1:] if importlib.util.find_spec(m) is None]
raise SystemExit(0 if not missing else 1)
'@

    & $venvPython -c $probeScript @Modules *> $null
    if ($LASTEXITCODE -ne 0) {
        throw "[python-runtime] $ServerName: packaged venv is missing required modules"
    }
}

$root = (Resolve-Path -LiteralPath $ArtifactRoot).Path
$restRoot = Join-Path $root "kaspa-rest-server"
$socketRoot = Join-Path $root "kaspa-socket-server"

Require-Directory $restRoot
Require-Directory $socketRoot
Require-File (Join-Path $restRoot "main.py")
Require-File (Join-Path $restRoot "pyproject.toml")
Require-File (Join-Path $socketRoot "main.py")
Require-File (Join-Path $socketRoot "Pipfile")

$pythonExe = Resolve-PythonExe
if (-not $pythonExe) {
    throw "No compatible Python runtime (>=3.10) found for self-hosted services"
}

$bootstrapScript = @'
import importlib.util
missing = [m for m in ("venv", "ensurepip") if importlib.util.find_spec(m) is None]
raise SystemExit(0 if not missing else 1)
'@
& $pythonExe -c $bootstrapScript *> $null
if ($LASTEXITCODE -ne 0) {
    throw "Python runtime is missing venv/ensurepip support: $pythonExe"
}

& $pythonExe -m venv --help *> $null
if ($LASTEXITCODE -ne 0) {
    throw "Python runtime cannot execute venv module: $pythonExe"
}

& $pythonExe -m pip --version *> $null
if ($LASTEXITCODE -ne 0) {
    & $pythonExe -m ensurepip --help *> $null
    if ($LASTEXITCODE -ne 0) {
        throw "Python runtime has neither pip nor ensurepip available: $pythonExe"
    }
}

Test-ServerVenvModules `
    -ServerName "kaspa-rest-server" `
    -ServerRoot $restRoot `
    -Modules @("fastapi", "uvicorn", "pydantic", "pydantic_settings", "asyncpg", "psycopg2")

Test-ServerVenvModules `
    -ServerName "kaspa-socket-server" `
    -ServerRoot $socketRoot `
    -Modules @("fastapi", "uvicorn", "pydantic", "pydantic_settings", "socketio", "engineio")

Write-Host "Self-hosted Python runtime verification passed: $root (python: $pythonExe)"
