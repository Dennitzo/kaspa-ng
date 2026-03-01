param(
    [string]$RepoRoot = "",
    [string]$OutDir = ""
)

$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}
if (-not $OutDir) {
    $OutDir = Join-Path $RepoRoot "target\release\postgres"
}

function Copy-Tree {
    param([string]$Src, [string]$Dst)
    if (-not (Test-Path -LiteralPath $Src)) { return $false }
    New-Item -ItemType Directory -Force -Path $Dst | Out-Null
    Copy-Item -Path (Join-Path $Src "*") -Destination $Dst -Recurse -Force
    return $true
}

function Resolve-PostgresRootFromExe {
    param([string]$ExePath)
    if (-not $ExePath) { return $null }
    $binDir = Split-Path -Parent $ExePath
    if (-not $binDir) { return $null }
    $root = Split-Path -Parent $binDir
    if (-not $root) { return $null }
    if (Test-Path -LiteralPath (Join-Path $root "bin\postgres.exe")) {
        return $root
    }
    return $null
}

function Get-InstalledPostgresRoots {
    $roots = New-Object System.Collections.Generic.List[string]
    $bases = @(
        "C:\Program Files\PostgreSQL",
        "C:\Program Files (x86)\PostgreSQL"
    )

    foreach ($base in $bases) {
        if (-not (Test-Path -LiteralPath $base)) { continue }
        foreach ($child in (Get-ChildItem -LiteralPath $base -Directory -ErrorAction SilentlyContinue)) {
            $exe = Join-Path $child.FullName "bin\postgres.exe"
            if (Test-Path -LiteralPath $exe) {
                $roots.Add($child.FullName) | Out-Null
            }
        }
    }

    $postgresCmd = Get-Command postgres -ErrorAction SilentlyContinue
    if ($postgresCmd -and $postgresCmd.Path) {
        $pathRoot = Resolve-PostgresRootFromExe -ExePath $postgresCmd.Path
        if ($pathRoot) {
            $roots.Add($pathRoot) | Out-Null
        }
    }

    $unique = $roots | Sort-Object -Unique
    $scored = foreach ($root in $unique) {
        $leaf = Split-Path -Leaf $root
        $major = 0
        if ($leaf -match '^\d+$') {
            $major = [int]$leaf
        }
        [PSCustomObject]@{
            Root = $root
            Major = $major
        }
    }

    return $scored `
        | Sort-Object -Property @{Expression = "Major"; Descending = $true}, @{Expression = "Root"; Descending = $false} `
        | Select-Object -ExpandProperty Root
}

if (Test-Path -LiteralPath $OutDir) {
    Remove-Item -LiteralPath $OutDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$sourceRoot = $null
$candidates = Get-InstalledPostgresRoots
if ($candidates.Count -gt 0) {
    $sourceRoot = $candidates[0]
}

if (-not $sourceRoot -and (Get-Command winget -ErrorAction SilentlyContinue)) {
    $wingetIds = @(
        "PostgreSQL.PostgreSQL.15",
        "PostgreSQL.PostgreSQL.16",
        "PostgreSQL.PostgreSQL.17",
        "PostgreSQL.PostgreSQL.14"
    )

    foreach ($id in $wingetIds) {
        & winget install -e --id $id --accept-package-agreements --accept-source-agreements --silent | Out-Null
        $candidates = Get-InstalledPostgresRoots
        if ($candidates.Count -gt 0) {
            $sourceRoot = $candidates[0]
            break
        }
    }
}

if (-not $sourceRoot) {
    throw "Unable to find a PostgreSQL installation to stage runtime (checked Program Files, PATH, and winget installs)."
}

$ok = $true
$ok = (Copy-Tree -Src (Join-Path $sourceRoot "bin") -Dst (Join-Path $OutDir "bin")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $sourceRoot "lib") -Dst (Join-Path $OutDir "lib")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $sourceRoot "share") -Dst (Join-Path $OutDir "share")) -and $ok

if (-not (Test-Path -LiteralPath (Join-Path $OutDir "bin\postgres.exe"))) {
    throw "Staged postgres runtime is invalid (bin\\postgres.exe missing)."
}

Write-Host "[postgres] staged runtime from '$sourceRoot' to '$OutDir'"
