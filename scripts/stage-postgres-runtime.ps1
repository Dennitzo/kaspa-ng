param(
    [string]$RepoRoot = "",
    [string]$OutDir = ""
)

$ErrorActionPreference = "Stop"
$ExpectedMajor = 15

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}
if (-not $OutDir) {
    $OutDir = Join-Path $RepoRoot "target\release\postgres"
}

$SourceRoot = Join-Path $RepoRoot "postgres"

function Copy-Tree {
    param([string]$Src, [string]$Dst)
    if (-not (Test-Path -LiteralPath $Src)) { return $false }
    New-Item -ItemType Directory -Force -Path $Dst | Out-Null
    Copy-Item -Path (Join-Path $Src "*") -Destination $Dst -Recurse -Force
    return $true
}

function Get-PostgresMajorVersion {
    param([string]$Root)
    $exe = Join-Path $Root "bin\postgres.exe"
    if (-not (Test-Path -LiteralPath $exe)) { return $null }
    try {
        $output = & $exe --version 2>$null
        if ($LASTEXITCODE -ne 0 -or -not $output) { return $null }
        $text = [string]::Join(" ", $output)
        $match = [regex]::Match($text, "\d+")
        if ($match.Success) {
            return [int]$match.Value
        }
    } catch {
        return $null
    }
    return $null
}

function Test-ExpectedRuntimeRoot {
    param([string]$Root)
    if (-not $Root) { return $false }

    $postgres = Join-Path $Root "bin\postgres.exe"
    $initdb = Join-Path $Root "bin\initdb.exe"
    $pgCtl = Join-Path $Root "bin\pg_ctl.exe"

    if (-not (Test-Path -LiteralPath $postgres)) { return $false }
    if (-not (Test-Path -LiteralPath $initdb)) { return $false }
    if (-not (Test-Path -LiteralPath $pgCtl)) { return $false }

    $major = Get-PostgresMajorVersion -Root $Root
    if ($major -ne $ExpectedMajor) { return $false }

    try {
        & $initdb --version *> $null
        if ($LASTEXITCODE -ne 0) { return $false }
        & $pgCtl --version *> $null
        if ($LASTEXITCODE -ne 0) { return $false }
    } catch {
        return $false
    }

    return $true
}

if (-not (Test-ExpectedRuntimeRoot -Root $SourceRoot)) {
    $fetchScript = Join-Path $RepoRoot "scripts\fetch-postgres-runtime.ps1"
    if (Test-Path -LiteralPath $fetchScript) {
        Write-Host "[postgres] bundled runtime missing or unsupported at '$SourceRoot'; attempting automatic fetch"
        & $fetchScript -RepoRoot $RepoRoot -ExpectedMajor $ExpectedMajor
    }
}

if (-not (Test-ExpectedRuntimeRoot -Root $SourceRoot)) {
    throw "Bundled PostgreSQL runtime missing or unsupported at '$SourceRoot' (expected major $ExpectedMajor with bin\postgres.exe, bin\initdb.exe, bin\pg_ctl.exe)."
}

if ([System.StringComparer]::OrdinalIgnoreCase.Equals($SourceRoot, $OutDir)) {
    Write-Host "[postgres] runtime already staged at '$OutDir' (major $ExpectedMajor)"
    exit 0
}

if (Test-Path -LiteralPath $OutDir) {
    Remove-Item -LiteralPath $OutDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$ok = $true
$ok = (Copy-Tree -Src (Join-Path $SourceRoot "bin") -Dst (Join-Path $OutDir "bin")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $SourceRoot "lib") -Dst (Join-Path $OutDir "lib")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $SourceRoot "share") -Dst (Join-Path $OutDir "share")) -and $ok

if (-not $ok -or -not (Test-ExpectedRuntimeRoot -Root $OutDir)) {
    throw "Unable to stage bundled PostgreSQL runtime from '$SourceRoot' to '$OutDir'."
}

Write-Host "[postgres] staged runtime from '$SourceRoot' to '$OutDir' (major $ExpectedMajor)"
