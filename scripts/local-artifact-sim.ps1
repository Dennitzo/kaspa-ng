[CmdletBinding()]
param(
    [string]$LogDir,
    [string]$ArtifactRoot,
    [switch]$SkipKasia,
    [switch]$SkipCargo,
    [switch]$SkipPackage,
    [switch]$EnableDebug,
    [switch]$NoDebug
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if ($PSVersionTable.PSVersion.Major -ge 7) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$RootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $RootDir

function Convert-ToBoolFromEnv {
    param(
        [string]$Value,
        [bool]$Default = $false
    )

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $Default
    }

    switch ($Value.Trim().ToLowerInvariant()) {
        "1" { return $true }
        "true" { return $true }
        "yes" { return $true }
        "on" { return $true }
        "0" { return $false }
        "false" { return $false }
        "no" { return $false }
        "off" { return $false }
        default { return $Default }
    }
}

if (-not $PSBoundParameters.ContainsKey("LogDir")) {
    if ($env:LOG_DIR) {
        $LogDir = $env:LOG_DIR
    } else {
        $LogDir = Join-Path $RootDir "ci-local-logs"
    }
}

if (-not $PSBoundParameters.ContainsKey("ArtifactRoot")) {
    $ArtifactRoot = $env:ARTIFACT_ROOT
}

$skipKasiaValue = $SkipKasia.IsPresent -or (Convert-ToBoolFromEnv -Value $env:SKIP_KASIA -Default $false)
$skipCargoValue = $SkipCargo.IsPresent -or (Convert-ToBoolFromEnv -Value $env:SKIP_CARGO -Default $false)
$skipPackageValue = $SkipPackage.IsPresent -or (Convert-ToBoolFromEnv -Value $env:SKIP_PACKAGE -Default $false)

$debugEnabled = if ($EnableDebug.IsPresent) {
    $true
} elseif ($NoDebug.IsPresent) {
    $false
} else {
    Convert-ToBoolFromEnv -Value $env:DEBUG -Default $true
}

if ($debugEnabled) {
    $VerbosePreference = "Continue"
}

if (-not (Test-Path -LiteralPath $LogDir)) {
    New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
}

function Require-Command {
    param([string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command not found: $Name"
    }
}

function Invoke-Native {
    param(
        [string]$File,
        [string[]]$Arguments = @(),
        [switch]$AllowFailure
    )

    $resolvedFile = $File
    switch ($File) {
        "npm" {
            if (Get-Command "npm.cmd" -ErrorAction SilentlyContinue) { $resolvedFile = "npm.cmd" }
        }
        "node" {
            if (Get-Command "node.exe" -ErrorAction SilentlyContinue) { $resolvedFile = "node.exe" }
        }
        "cargo" {
            if (Get-Command "cargo.exe" -ErrorAction SilentlyContinue) { $resolvedFile = "cargo.exe" }
        }
        "git" {
            if (Get-Command "git.exe" -ErrorAction SilentlyContinue) { $resolvedFile = "git.exe" }
        }
    }

    & $resolvedFile @Arguments
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0 -and -not $AllowFailure.IsPresent) {
        throw "Command failed (exit $exitCode): $resolvedFile $($Arguments -join ' ')"
    }
    return $exitCode
}

function Copy-DirContent {
    param(
        [string]$Source,
        [string]$Destination
    )

    if (-not (Test-Path -LiteralPath $Source)) {
        return
    }

    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    $items = Get-ChildItem -LiteralPath $Source -Force
    if ($null -eq $items -or $items.Count -eq 0) {
        return
    }
    Copy-Item -LiteralPath $items.FullName -Destination $Destination -Recurse -Force
}

function Sync-ExternalRepo {
    param(
        [string]$Dir,
        [string]$Url
    )

    $target = Join-Path $RootDir $Dir
    $gitDir = Join-Path $target ".git"

    if (Test-Path -LiteralPath $gitDir) {
        $currentUrl = ""
        try {
            $currentUrl = (& git -C $target remote get-url origin).Trim()
        } catch {
            $currentUrl = ""
        }

        if (-not [string]::IsNullOrWhiteSpace($currentUrl) -and $currentUrl -ne $Url) {
            Write-Host "External repo remote mismatch for $Dir; recloning ($currentUrl -> $Url)"
            Remove-Item -LiteralPath $target -Recurse -Force
            Invoke-Native -File "git" -Arguments @("clone", "--depth", "1", $Url, $target) | Out-Null
            return
        }

        Write-Host "Updating external repo $Dir via git pull --ff-only"
        Invoke-Native -File "git" -Arguments @("-C", $target, "pull", "--ff-only") | Out-Null
        return
    }

    if (Test-Path -LiteralPath $target) {
        Write-Host "External repo $Dir exists without .git; recloning"
        Remove-Item -LiteralPath $target -Recurse -Force
    }

    Write-Host "Cloning external repo $Dir"
    Invoke-Native -File "git" -Arguments @("clone", "--depth", "1", $Url, $target) | Out-Null
}

function Sync-ExternalRepos {
    $repos = @(
        @{ Dir = "rusty-kaspa"; Url = "https://github.com/kaspanet/rusty-kaspa.git" },
        @{ Dir = "K"; Url = "https://github.com/thesheepcat/K.git" },
        @{ Dir = "K-indexer"; Url = "https://github.com/thesheepcat/K-indexer.git" },
        @{ Dir = "simply-kaspa-indexer"; Url = "https://github.com/supertypo/simply-kaspa-indexer.git" },
        @{ Dir = "kasia-indexer"; Url = "https://github.com/K-Kluster/kasia-indexer.git" },
        @{ Dir = "Kasia"; Url = "https://github.com/K-Kluster/Kasia.git" },
        @{ Dir = "kasvault"; Url = "https://github.com/coderofstuff/kasvault.git" }
    )

    foreach ($repo in $repos) {
        Sync-ExternalRepo -Dir $repo.Dir -Url $repo.Url
    }
}

function Is-CompatibleKaspaWasmDir {
    param([string]$Dir)

    $pkg = Join-Path $Dir "package.json"
    $js = Join-Path $Dir "kaspa.js"
    $dts = Join-Path $Dir "kaspa.d.ts"

    if (-not (Test-Path -LiteralPath $pkg) -or -not (Test-Path -LiteralPath $js) -or -not (Test-Path -LiteralPath $dts)) {
        return $false
    }

    $pkgContent = Get-Content -LiteralPath $pkg -Raw
    $jsContent = Get-Content -LiteralPath $js -Raw

    if ($pkgContent -notmatch '"name"\s*:\s*"kaspa-wasm"') { return $false }
    if ($jsContent -notmatch 'export\s+default') { return $false }
    if ($jsContent -notmatch 'export\s+class\s+RpcClient') { return $false }
    if ($jsContent -notmatch 'export\s+const\s+ConnectStrategy') { return $false }
    if ($jsContent -notmatch 'export\s+const\s+Encoding') { return $false }
    if ($jsContent -notmatch 'export\s+class\s+Resolver') { return $false }
    if ($jsContent -notmatch 'export\s+function\s+initConsolePanicHook') { return $false }

    return $true
}

function Find-KaspaWasmDir {
    param([string]$SearchRoot)

    if (-not (Test-Path -LiteralPath $SearchRoot)) {
        return $null
    }

    $directCandidates = @(
        (Join-Path $SearchRoot "kaspa-wasm32-sdk\web\kaspa"),
        (Join-Path $SearchRoot "web\kaspa")
    )

    foreach ($candidate in $directCandidates) {
        if (Is-CompatibleKaspaWasmDir -Dir $candidate) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    $pkgs = Get-ChildItem -Path $SearchRoot -Recurse -File -Filter package.json -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '[\\/]web[\\/]kaspa[\\/]package\.json$' }

    foreach ($pkg in $pkgs) {
        $parent = Split-Path -Parent $pkg.FullName
        if (Is-CompatibleKaspaWasmDir -Dir $parent) {
            return $parent
        }
    }

    return $null
}

function Copy-KasiaWasmFrom {
    param([string]$SourceDir)

    Write-Host "Using kaspa-wasm package from: $SourceDir"

    $target = Join-Path $RootDir "Kasia\wasm"
    if (Test-Path -LiteralPath $target) {
        Remove-Item -LiteralPath $target -Recurse -Force
    }

    New-Item -ItemType Directory -Force -Path $target | Out-Null
    Copy-DirContent -Source $SourceDir -Destination $target

    if (-not (Is-CompatibleKaspaWasmDir -Dir $target)) {
        throw "Copied Kasia/wasm package is incompatible"
    }
}

function Prepare-KasiaWasm {
    $kasiaDir = Join-Path $RootDir "Kasia"
    if (-not (Test-Path -LiteralPath $kasiaDir)) {
        return
    }

    $wasmDir = Join-Path $kasiaDir "wasm"
    if (Is-CompatibleKaspaWasmDir -Dir $wasmDir) {
        Write-Verbose "Kasia wasm package already present and compatible"
        return
    }

    if (Test-Path -LiteralPath $wasmDir) {
        Remove-Item -LiteralPath $wasmDir -Recurse -Force
    }

    $pkgDir = Find-KaspaWasmDir -SearchRoot (Join-Path $RootDir "rusty-kaspa")
    if ($pkgDir) {
        Copy-KasiaWasmFrom -SourceDir $pkgDir
        return
    }

    $tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("kasiawasm-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

    try {
        if ($env:KASIA_WASM_SDK_URL) {
            $urls = @($env:KASIA_WASM_SDK_URL)
        } else {
            $urls = @(
                "https://github.com/IzioDev/rusty-kaspa/releases/download/v1.0.1-beta1/kaspa-wasm32-sdk-v1.0.1-beta1.zip",
                "https://github.com/kaspanet/rusty-kaspa/releases/download/v1.0.0/kaspa-wasm32-sdk-v1.0.0.zip"
            )
        }

        foreach ($url in $urls) {
            Write-Host "Attempting Kasia wasm SDK download from $url"
            $zipPath = Join-Path $tmpRoot "sdk.zip"
            $extractDir = Join-Path $tmpRoot "extract"

            if (Test-Path -LiteralPath $zipPath) {
                Remove-Item -LiteralPath $zipPath -Force
            }
            if (Test-Path -LiteralPath $extractDir) {
                Remove-Item -LiteralPath $extractDir -Recurse -Force
            }

            try {
                Invoke-WebRequest -Uri $url -OutFile $zipPath -MaximumRedirection 5
            } catch {
                Write-Warning "Download failed: $url"
                continue
            }

            Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force
            $pkgDir = Find-KaspaWasmDir -SearchRoot $extractDir
            if ($pkgDir) {
                Copy-KasiaWasmFrom -SourceDir $pkgDir
                return
            }
        }
    }
    finally {
        if (Test-Path -LiteralPath $tmpRoot) {
            Remove-Item -LiteralPath $tmpRoot -Recurse -Force
        }
    }

    throw "Unable to locate compatible kaspa-wasm package for Kasia/wasm"
}

function Ensure-RollupNative {
    $nativeJs = Join-Path $RootDir "node_modules\rollup\dist\native.js"
    if (-not (Test-Path -LiteralPath $nativeJs)) {
        return
    }

    Invoke-Native -File "node" -Arguments @("-e", "require('rollup/dist/native.js')") -AllowFailure | Out-Null
    if ($LASTEXITCODE -eq 0) {
        return
    }

    $pkg = switch -Regex ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
        "Arm64" { "@rollup/rollup-win32-arm64-msvc"; break }
        default { "@rollup/rollup-win32-x64-msvc"; break }
    }

    Write-Host "rollup native binding missing; installing $pkg"
    Invoke-Native -File "npm" -Arguments @("install", "--no-audit", "--no-fund", "--no-save", $pkg) | Out-Null
    Invoke-Native -File "node" -Arguments @("-e", "require('rollup/dist/native.js')") | Out-Null
}

function Npm-InstallWithFallback {
    if (Test-Path -LiteralPath "package-lock.json") {
        $code = Invoke-Native -File "npm" -Arguments @("ci", "--prefer-offline", "--no-audit", "--no-fund") -AllowFailure
        if ($code -ne 0) {
            Invoke-Native -File "npm" -Arguments @("install", "--no-audit", "--no-fund") | Out-Null
        }
    }
    else {
        Invoke-Native -File "npm" -Arguments @("install", "--no-audit", "--no-fund") | Out-Null
    }

    Ensure-RollupNative
}

function Ensure-WasmPack {
    if (Get-Command "wasm-pack" -ErrorAction SilentlyContinue) {
        return
    }

    Write-Host "wasm-pack not found; installing via cargo"
    Invoke-Native -File "cargo" -Arguments @("install", "wasm-pack", "--locked") | Out-Null
    if (-not (Get-Command "wasm-pack" -ErrorAction SilentlyContinue)) {
        throw "Failed to install wasm-pack"
    }
}

function Build-Kasia {
    if (-not (Test-Path -LiteralPath (Join-Path $RootDir "Kasia"))) {
        return
    }

    Ensure-WasmPack

    Push-Location (Join-Path $RootDir "Kasia")
    try {
        Npm-InstallWithFallback
        Invoke-Native -File "npm" -Arguments @("run", "wasm:build") | Out-Null

        $code = Invoke-Native -File "npm" -Arguments @("run", "build:production") -AllowFailure
        if ($code -ne 0) {
            Invoke-Native -File "npm" -Arguments @("exec", "vite", "build") | Out-Null
        }
    }
    finally {
        Pop-Location
    }
}

function Build-KIfMissing {
    $hasRelease = Test-Path -LiteralPath (Join-Path $RootDir "target\release\K\dist")
    $hasLocal = Test-Path -LiteralPath (Join-Path $RootDir "K\dist")
    $exists = Test-Path -LiteralPath (Join-Path $RootDir "K")

    if ($hasRelease -or $hasLocal -or -not $exists) {
        return
    }

    Write-Host "K dist missing after cargo build; running fallback build"
    Push-Location (Join-Path $RootDir "K")
    try {
        Npm-InstallWithFallback
        Invoke-Native -File "npm" -Arguments @("run", "build") | Out-Null
    }
    finally {
        Pop-Location
    }
}

function Build-KasiaIfMissing {
    $hasRelease = Test-Path -LiteralPath (Join-Path $RootDir "target\release\Kasia\dist")
    $hasLocal = Test-Path -LiteralPath (Join-Path $RootDir "Kasia\dist")
    $exists = Test-Path -LiteralPath (Join-Path $RootDir "Kasia")

    if ($hasRelease -or $hasLocal -or -not $exists) {
        return
    }

    Write-Host "Kasia dist missing after cargo build; running fallback build"
    Push-Location (Join-Path $RootDir "Kasia")
    try {
        Npm-InstallWithFallback
        Invoke-Native -File "npm" -Arguments @("run", "wasm:build") -AllowFailure | Out-Null
        $code = Invoke-Native -File "npm" -Arguments @("run", "build:production") -AllowFailure
        if ($code -ne 0) {
            Invoke-Native -File "npm" -Arguments @("exec", "vite", "build") | Out-Null
        }
    }
    finally {
        Pop-Location
    }
}

function Build-KasVaultIfMissing {
    $hasRelease = Test-Path -LiteralPath (Join-Path $RootDir "target\release\KasVault\build")
    $hasLocal = Test-Path -LiteralPath (Join-Path $RootDir "kasvault\build")
    $exists = Test-Path -LiteralPath (Join-Path $RootDir "kasvault")

    if ($hasRelease -or $hasLocal -or -not $exists) {
        return
    }

    Write-Host "KasVault build missing after cargo build; running fallback build"
    Push-Location (Join-Path $RootDir "kasvault")
    try {
        Npm-InstallWithFallback
        Invoke-Native -File "npm" -Arguments @("run", "build") | Out-Null
    }
    finally {
        Pop-Location
    }
}

function Build-ExplorerIfMissing {
    $hasRelease = Test-Path -LiteralPath (Join-Path $RootDir "target\release\kaspa-explorer-ng")
    $hasLocal = Test-Path -LiteralPath (Join-Path $RootDir "kaspa-explorer-ng\build")
    $exists = Test-Path -LiteralPath (Join-Path $RootDir "kaspa-explorer-ng")

    if ($hasRelease -or $hasLocal -or -not $exists) {
        return
    }

    Write-Host "kaspa-explorer-ng build missing after cargo build; running fallback build"
    Push-Location (Join-Path $RootDir "kaspa-explorer-ng")
    try {
        Npm-InstallWithFallback
        Invoke-Native -File "npm" -Arguments @("run", "build") | Out-Null
    }
    finally {
        Pop-Location
    }
}

function Build-FrontendsIfMissing {
    Build-KIfMissing
    Build-KasiaIfMissing
    Build-KasVaultIfMissing
    Build-ExplorerIfMissing
}

function Build-Release {
    $previous = $env:KASIA_WASM_AUTO_FETCH
    $env:KASIA_WASM_AUTO_FETCH = "0"
    try {
        Invoke-Native -File "cargo" -Arguments @("build", "--release") | Out-Null
    }
    finally {
        if ($null -eq $previous) {
            Remove-Item Env:KASIA_WASM_AUTO_FETCH -ErrorAction SilentlyContinue
        } else {
            $env:KASIA_WASM_AUTO_FETCH = $previous
        }
    }
}

function Stage-PostgresRuntime {
    $stageScript = Join-Path $RootDir "scripts\stage-postgres-runtime.ps1"
    if (-not (Test-Path -LiteralPath $stageScript)) {
        return
    }

    $psExe = if (Get-Command "pwsh" -ErrorAction SilentlyContinue) { "pwsh" } else { "powershell.exe" }
    Invoke-Native -File $psExe -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $stageScript, "-RepoRoot", $RootDir, "-OutDir", "target\release\postgres") | Out-Null
}

function Copy-BinaryIfExists {
    param(
        [string]$Bin,
        [string]$Destination
    )

    $candidates = @(
        (Join-Path $RootDir ("target\release\{0}" -f $Bin)),
        (Join-Path $RootDir ("rusty-kaspa\target\release\{0}" -f $Bin)),
        (Join-Path $RootDir ("simply-kaspa-indexer\target\release\{0}" -f $Bin)),
        (Join-Path $RootDir ("K-indexer\target\release\{0}" -f $Bin))
    )

    if ($Bin -eq "kasia-indexer.exe") {
        $candidates += (Join-Path $RootDir "kasia-indexer\target\release\indexer.exe")
    }

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            $fileName = [System.IO.Path]::GetFileName($candidate)
            $outName = if ($Bin -eq "kasia-indexer.exe") { "kasia-indexer.exe" } else { $fileName }
            Copy-Item -LiteralPath $candidate -Destination (Join-Path $Destination $outName) -Force
            return
        }
    }
}

function Package-AndVerify {
    Build-FrontendsIfMissing

    $shortSha = "local"
    try {
        $shaOut = (& git rev-parse --short HEAD).Trim()
        if ($LASTEXITCODE -eq 0 -and $shaOut) {
            $shortSha = $shaOut
        }
    } catch {
        $shortSha = "local"
    }

    $platform = "windows-x64"
    $root = if ([string]::IsNullOrWhiteSpace($ArtifactRoot)) {
        "kaspa-ng-$shortSha-$platform-local-sim"
    } else {
        $ArtifactRoot
    }

    $rootPath = Join-Path $RootDir $root

    if (Test-Path -LiteralPath $rootPath) {
        Remove-Item -LiteralPath $rootPath -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $rootPath | Out-Null

    $mainExe = Join-Path $RootDir "target\release\kaspa-ng.exe"
    if (-not (Test-Path -LiteralPath $mainExe)) {
        throw "Missing binary: target\\release\\kaspa-ng.exe"
    }
    Copy-Item -LiteralPath $mainExe -Destination (Join-Path $rootPath "kaspa-ng.exe") -Force

    foreach ($bin in @(
            "stratum-bridge.exe",
            "simply-kaspa-indexer.exe",
            "K-webserver.exe",
            "K-transaction-processor.exe",
            "kasia-indexer.exe"
        )) {
        Copy-BinaryIfExists -Bin $bin -Destination $rootPath
    }

    $explorerRelease = Join-Path $RootDir "target\release\kaspa-explorer-ng"
    $explorerLocal = Join-Path $RootDir "kaspa-explorer-ng\build"
    if (Test-Path -LiteralPath $explorerRelease) {
        Copy-Item -LiteralPath $explorerRelease -Destination (Join-Path $rootPath "kaspa-explorer-ng") -Recurse -Force
    }
    elseif (Test-Path -LiteralPath $explorerLocal) {
        $dst = Join-Path $rootPath "kaspa-explorer-ng"
        New-Item -ItemType Directory -Force -Path $dst | Out-Null
        Copy-Item -LiteralPath $explorerLocal -Destination (Join-Path $dst "build") -Recurse -Force
    }

    foreach ($dir in @("kaspa-rest-server", "kaspa-socket-server", "Loader")) {
        $src = Join-Path $RootDir $dir
        if (Test-Path -LiteralPath $src) {
            Copy-Item -LiteralPath $src -Destination (Join-Path $rootPath $dir) -Recurse -Force
        }
    }

    $postgresRelease = Join-Path $RootDir "target\release\postgres"
    if (-not (Test-Path -LiteralPath $postgresRelease)) {
        throw "Missing staged PostgreSQL runtime: target\release\postgres"
    }
    Copy-Item -LiteralPath $postgresRelease -Destination (Join-Path $rootPath "postgres") -Recurse -Force

    $kRelease = Join-Path $RootDir "target\release\K\dist"
    $kLocal = Join-Path $RootDir "K\dist"
    if (Test-Path -LiteralPath $kRelease) {
        $dst = Join-Path $rootPath "K"
        New-Item -ItemType Directory -Force -Path $dst | Out-Null
        Copy-Item -LiteralPath $kRelease -Destination (Join-Path $dst "dist") -Recurse -Force
    }
    elseif (Test-Path -LiteralPath $kLocal) {
        $dst = Join-Path $rootPath "K"
        New-Item -ItemType Directory -Force -Path $dst | Out-Null
        Copy-Item -LiteralPath $kLocal -Destination (Join-Path $dst "dist") -Recurse -Force
    }

    $kasiaRelease = Join-Path $RootDir "target\release\Kasia\dist"
    $kasiaLocal = Join-Path $RootDir "Kasia\dist"
    if (Test-Path -LiteralPath $kasiaRelease) {
        $dst = Join-Path $rootPath "Kasia"
        New-Item -ItemType Directory -Force -Path $dst | Out-Null
        Copy-Item -LiteralPath $kasiaRelease -Destination (Join-Path $dst "dist") -Recurse -Force
    }
    elseif (Test-Path -LiteralPath $kasiaLocal) {
        $dst = Join-Path $rootPath "Kasia"
        New-Item -ItemType Directory -Force -Path $dst | Out-Null
        Copy-Item -LiteralPath $kasiaLocal -Destination (Join-Path $dst "dist") -Recurse -Force
    }

    $kasVaultRelease = Join-Path $RootDir "target\release\KasVault\build"
    $kasVaultLocal = Join-Path $RootDir "kasvault\build"
    if (Test-Path -LiteralPath $kasVaultRelease) {
        $dst = Join-Path $rootPath "KasVault"
        New-Item -ItemType Directory -Force -Path $dst | Out-Null
        Copy-Item -LiteralPath $kasVaultRelease -Destination (Join-Path $dst "build") -Recurse -Force
    }
    elseif (Test-Path -LiteralPath $kasVaultLocal) {
        $dst = Join-Path $rootPath "KasVault"
        New-Item -ItemType Directory -Force -Path $dst | Out-Null
        Copy-Item -LiteralPath $kasVaultLocal -Destination (Join-Path $dst "build") -Recurse -Force
    }

    foreach ($file in @(
            "kaspa-ng.exe",
            "stratum-bridge.exe",
            "simply-kaspa-indexer.exe",
            "K-webserver.exe",
            "K-transaction-processor.exe",
            "kasia-indexer.exe"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $rootPath $file))) {
            throw "Missing packaged binary: $file"
        }
    }

    foreach ($dir in @("kaspa-explorer-ng", "kaspa-rest-server", "kaspa-socket-server", "Loader", "K", "Kasia", "KasVault", "postgres")) {
        if (-not (Test-Path -LiteralPath (Join-Path $rootPath $dir))) {
            throw "Missing packaged directory: $dir"
        }
    }

    $pythonVerifyScript = Join-Path $RootDir "scripts\verify-self-hosted-python-runtime.ps1"
    & $pythonVerifyScript -ArtifactRoot $rootPath
    if ($LASTEXITCODE -ne 0) {
        throw "Self-hosted Python runtime verification failed for: $rootPath"
    }

    if (-not (Test-Path -LiteralPath (Join-Path $rootPath "postgres\bin\postgres.exe"))) {
        throw "Missing PostgreSQL runtime binary in packaged layout (postgres.exe)"
    }
    if (-not (Test-Path -LiteralPath (Join-Path $rootPath "postgres\bin\initdb.exe"))) {
        throw "Missing PostgreSQL runtime binary in packaged layout (initdb.exe)"
    }
    if (-not (Test-Path -LiteralPath (Join-Path $rootPath "postgres\bin\pg_ctl.exe"))) {
        throw "Missing PostgreSQL runtime binary in packaged layout (pg_ctl.exe)"
    }

    Write-Host "LOCAL_ARTIFACT_SIM_OK root=$root"
}

function Invoke-LoggedStage {
    param(
        [string]$Description,
        [string]$LogFile,
        [scriptblock]$Action
    )

    Write-Host $Description
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $Action 2>&1 | Tee-Object -FilePath $LogFile
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
}

Require-Command -Name "git"
Require-Command -Name "cargo"
Require-Command -Name "npm"
Require-Command -Name "node"
if (-not (Get-Command "pwsh" -ErrorAction SilentlyContinue) -and -not (Get-Command "powershell.exe" -ErrorAction SilentlyContinue)) {
    throw "Required command not found: pwsh or powershell.exe"
}

try {
    Invoke-LoggedStage -Description "==> [0/5] Sync external repositories" -LogFile (Join-Path $LogDir "external-repo-sync.log") -Action {
        Sync-ExternalRepos
    }

    Invoke-LoggedStage -Description "==> [1/5] Prepare Kasia wasm package" -LogFile (Join-Path $LogDir "prepare-kasia-wasm.log") -Action {
        Prepare-KasiaWasm
    }

    if (-not $skipKasiaValue) {
        Invoke-LoggedStage -Description "==> [2/5] Build Kasia frontend" -LogFile (Join-Path $LogDir "kasia-build.log") -Action {
            Build-Kasia
        }
    }
    else {
        Write-Host "==> [2/5] Skipped Kasia build"
    }

    if (-not $skipCargoValue) {
        Invoke-LoggedStage -Description "==> [3/5] Cargo release build" -LogFile (Join-Path $LogDir "cargo-build-release.log") -Action {
            Build-Release
        }

        Invoke-LoggedStage -Description "==> [3b/5] Stage internal PostgreSQL runtime" -LogFile (Join-Path $LogDir "postgres-runtime-stage.log") -Action {
            Stage-PostgresRuntime
        }
    }
    else {
        Write-Host "==> [3/5] Skipped cargo build"
    }

    if (-not $skipPackageValue) {
        Invoke-LoggedStage -Description "==> [4/5] Package + verify artifact layout" -LogFile (Join-Path $LogDir "package-verify.log") -Action {
            Package-AndVerify
        }
    }
    else {
        Write-Host "==> [4/5] Skipped package/verify"
    }

    Write-Host "Done. Logs in: $LogDir"
}
catch {
    Write-Error "ERROR: $($_.Exception.Message)"
    exit 1
}
