param(
    [string]$RepoRoot = "",
    [int]$ExpectedMajor = 15,
    [string]$Version = "15.17-1",
    [string]$Url = "",
    [string]$Sha256 = "",
    [switch]$Force
)

$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}

$SourceRoot = Join-Path $RepoRoot "postgres"

if (-not $Url) {
    if ($env:KASPA_NG_POSTGRES_WINDOWS_URL) {
        $Url = $env:KASPA_NG_POSTGRES_WINDOWS_URL
    } else {
        $Url = "https://get.enterprisedb.com/postgresql/postgresql-$Version-windows-x64-binaries.zip"
    }
}

if (-not $Sha256) {
    if ($env:KASPA_NG_POSTGRES_WINDOWS_SHA256) {
        $Sha256 = $env:KASPA_NG_POSTGRES_WINDOWS_SHA256
    } elseif ($Version -eq "15.17-1") {
        $Sha256 = "048c6366830c3b6b62d466954468ee3c03ec37ba679ea1726c5bd20582dd11f8"
    }
}

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
    param(
        [string]$Root,
        [int]$Major
    )
    if (-not $Root) { return $false }

    $postgres = Join-Path $Root "bin\postgres.exe"
    $initdb = Join-Path $Root "bin\initdb.exe"
    $pgCtl = Join-Path $Root "bin\pg_ctl.exe"

    if (-not (Test-Path -LiteralPath $postgres)) { return $false }
    if (-not (Test-Path -LiteralPath $initdb)) { return $false }
    if (-not (Test-Path -LiteralPath $pgCtl)) { return $false }

    $major = Get-PostgresMajorVersion -Root $Root
    if ($major -ne $Major) { return $false }

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

if (-not $Force -and (Test-ExpectedRuntimeRoot -Root $SourceRoot -Major $ExpectedMajor)) {
    Write-Host "[postgres] bundled runtime already available at '$SourceRoot' (major $ExpectedMajor)"
    exit 0
}

$tmpRoot = Join-Path $env:TEMP "kaspa-ng-postgres-runtime-$ExpectedMajor"
$archivePath = Join-Path $tmpRoot "postgresql-$Version-windows-x64-binaries.zip"
$extractRoot = Join-Path $tmpRoot "extract"

if (Test-Path -LiteralPath $extractRoot) {
    Remove-Item -LiteralPath $extractRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null
New-Item -ItemType Directory -Force -Path $extractRoot | Out-Null

Write-Host "[postgres] downloading bundled runtime from '$Url'"
if (Get-Command "curl.exe" -ErrorAction SilentlyContinue) {
    & curl.exe -L --retry 5 --retry-delay 2 --output $archivePath $Url
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to download PostgreSQL runtime archive from '$Url'."
    }
} else {
    Invoke-WebRequest -Uri $Url -OutFile $archivePath
}

if ($Sha256) {
    $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
    $expectedHash = $Sha256.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) {
        throw "Checksum mismatch for PostgreSQL runtime archive. expected=$expectedHash actual=$actualHash"
    }
}

& tar.exe -xf $archivePath -C $extractRoot
if ($LASTEXITCODE -ne 0) {
    throw "Failed to extract '$archivePath'."
}

$payloadRoot = Join-Path $extractRoot "pgsql"
if (-not (Test-Path -LiteralPath $payloadRoot)) {
    $candidate = Get-ChildItem -LiteralPath $extractRoot -Directory -Recurse |
        Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName "bin\postgres.exe") } |
        Select-Object -First 1
    if (-not $candidate) {
        throw "Unable to locate extracted PostgreSQL payload root in '$extractRoot'."
    }
    $payloadRoot = $candidate.FullName
}

if (Test-Path -LiteralPath $SourceRoot) {
    Remove-Item -LiteralPath $SourceRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $SourceRoot | Out-Null

$ok = $true
$ok = (Copy-Tree -Src (Join-Path $payloadRoot "bin") -Dst (Join-Path $SourceRoot "bin")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $payloadRoot "lib") -Dst (Join-Path $SourceRoot "lib")) -and $ok
$ok = (Copy-Tree -Src (Join-Path $payloadRoot "share") -Dst (Join-Path $SourceRoot "share")) -and $ok

if (-not $ok -or -not (Test-ExpectedRuntimeRoot -Root $SourceRoot -Major $ExpectedMajor)) {
    throw "Downloaded PostgreSQL runtime is missing required files or has an unsupported major version."
}

Write-Host "[postgres] bundled runtime prepared at '$SourceRoot' (major $ExpectedMajor)"
