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

function Test-UsablePostgresRoot {
    param([string]$Root)
    if (-not $Root) { return $false }
    $exe = Join-Path $Root "bin\postgres.exe"
    if (-not (Test-Path -LiteralPath $exe)) { return $false }
    try {
        & $exe --version *> $null
        return ($LASTEXITCODE -eq 0)
    } catch {
        return $false
    }
}

function Try-StagePostgresRuntime {
    param(
        [string]$SourceRoot,
        [string]$TargetOutDir
    )

    if (Test-Path -LiteralPath $TargetOutDir) {
        Remove-Item -LiteralPath $TargetOutDir -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $TargetOutDir | Out-Null

    $ok = $true
    $ok = (Copy-Tree -Src (Join-Path $SourceRoot "bin") -Dst (Join-Path $TargetOutDir "bin")) -and $ok
    $ok = (Copy-Tree -Src (Join-Path $SourceRoot "lib") -Dst (Join-Path $TargetOutDir "lib")) -and $ok
    $ok = (Copy-Tree -Src (Join-Path $SourceRoot "share") -Dst (Join-Path $TargetOutDir "share")) -and $ok

    if (-not $ok) { return $false }
    if (-not (Test-Path -LiteralPath (Join-Path $TargetOutDir "bin\postgres.exe"))) { return $false }
    return $true
}

$candidateSet = New-Object System.Collections.Generic.HashSet[string]([System.StringComparer]::OrdinalIgnoreCase)
$orderedCandidates = New-Object System.Collections.Generic.List[string]

foreach ($candidate in (Get-InstalledPostgresRoots)) {
    if ($candidateSet.Add($candidate)) {
        $orderedCandidates.Add($candidate) | Out-Null
    }
}

if ($orderedCandidates.Count -eq 0 -and (Get-Command winget -ErrorAction SilentlyContinue)) {
    $wingetIds = @(
        "PostgreSQL.PostgreSQL.15",
        "PostgreSQL.PostgreSQL.16",
        "PostgreSQL.PostgreSQL.17",
        "PostgreSQL.PostgreSQL.14"
    )

    foreach ($id in $wingetIds) {
        & winget install -e --id $id --accept-package-agreements --accept-source-agreements --silent | Out-Null
        foreach ($candidate in (Get-InstalledPostgresRoots)) {
            if ($candidateSet.Add($candidate)) {
                $orderedCandidates.Add($candidate) | Out-Null
            }
        }
        if ($orderedCandidates.Count -gt 0) {
            break
        }
    }
}

if ($orderedCandidates.Count -eq 0) {
    throw "Unable to find a PostgreSQL installation to stage runtime (checked Program Files, PATH, and winget installs)."
}

$sourceRoot = $null
foreach ($candidate in $orderedCandidates) {
    if (-not (Test-UsablePostgresRoot -Root $candidate)) {
        Write-Host "[postgres] skipping unusable installation '$candidate'"
        continue
    }
    if (Try-StagePostgresRuntime -SourceRoot $candidate -TargetOutDir $OutDir) {
        $sourceRoot = $candidate
        break
    }
    Write-Host "[postgres] failed to stage runtime from '$candidate', trying next candidate"
}

if (-not $sourceRoot) {
    throw "Staged postgres runtime is invalid (bin\\postgres.exe missing). No usable installation candidates succeeded."
}

Write-Host "[postgres] staged runtime from '$sourceRoot' to '$OutDir'"
