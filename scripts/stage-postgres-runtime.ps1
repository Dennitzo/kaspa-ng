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

if (Test-Path -LiteralPath $OutDir) {
    Remove-Item -LiteralPath $OutDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$candidates = @(
    "C:\Program Files\PostgreSQL\15",
    "C:\Program Files (x86)\PostgreSQL\15"
)

$sourceRoot = $null
foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath (Join-Path $candidate "bin\postgres.exe")) {
        $sourceRoot = $candidate
        break
    }
}

if (-not $sourceRoot) {
    if (Get-Command winget -ErrorAction SilentlyContinue) {
        & winget install -e --id PostgreSQL.PostgreSQL.15 --accept-package-agreements --accept-source-agreements --silent | Out-Null
        foreach ($candidate in $candidates) {
            if (Test-Path -LiteralPath (Join-Path $candidate "bin\postgres.exe")) {
                $sourceRoot = $candidate
                break
            }
        }
    }
}

if (-not $sourceRoot) {
    throw "Unable to find PostgreSQL 15 installation to stage runtime."
}

$ok = $true
$ok = (Copy-Tree -Src (Join-Path $sourceRoot "bin") -Dst (Join-Path $OutDir "bin")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $sourceRoot "lib") -Dst (Join-Path $OutDir "lib")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $sourceRoot "share") -Dst (Join-Path $OutDir "share")) -and $ok

if (-not (Test-Path -LiteralPath (Join-Path $OutDir "bin\postgres.exe"))) {
    throw "Staged postgres runtime is invalid (bin\\postgres.exe missing)."
}

Write-Host "[postgres] staged runtime from '$sourceRoot' to '$OutDir'"