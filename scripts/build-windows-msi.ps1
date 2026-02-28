param(
    [string]$RepoRoot = "",
    [string]$PackageDir = "",
    [string]$Version = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

function Write-Info {
    param([string]$Message)
    Write-Host "[msi] $Message"
}

function Get-RepoRoot {
    if ($RepoRoot -and (Test-Path -LiteralPath $RepoRoot)) {
        return (Resolve-Path -LiteralPath $RepoRoot).Path
    }
    return (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
}

function Get-WorkspaceVersion {
    param([string]$Root)
    $cargoToml = Join-Path $Root "Cargo.toml"
    if (-not (Test-Path -LiteralPath $cargoToml)) {
        return "0.0.0"
    }

    $inWorkspacePackage = $false
    foreach ($line in Get-Content -LiteralPath $cargoToml) {
        $trimmed = $line.Trim()
        if ($trimmed -eq "[workspace.package]") {
            $inWorkspacePackage = $true
            continue
        }
        if ($inWorkspacePackage -and $trimmed.StartsWith("[")) {
            break
        }
        if ($inWorkspacePackage -and $trimmed -match '^version\s*=\s*"([^"]+)"') {
            return $matches[1]
        }
    }
    return "0.0.0"
}

function Normalize-MsiVersion {
    param([string]$RawVersion)
    $clean = $RawVersion
    if ($clean -match '^([0-9]+)\.([0-9]+)\.([0-9]+)') {
        return "$($matches[1]).$($matches[2]).$($matches[3])"
    }
    if ($clean -match '^([0-9]+)\.([0-9]+)') {
        return "$($matches[1]).$($matches[2]).0"
    }
    if ($clean -match '^([0-9]+)') {
        return "$($matches[1]).0.0"
    }
    return "0.0.0"
}

function Ensure-Wix {
    if (Get-Command wix -ErrorAction SilentlyContinue) {
        return
    }

    Write-Info "WiX CLI not found. Installing dotnet tool 'wix'..."
    dotnet tool install --global wix --version 4.*

    $dotnetTools = Join-Path $env:USERPROFILE ".dotnet\tools"
    if (Test-Path -LiteralPath $dotnetTools -and -not (($env:PATH -split ';') -contains $dotnetTools)) {
        $env:PATH = "$dotnetTools;$env:PATH"
    }

    if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
        throw "WiX CLI could not be resolved after installation."
    }
}

function Resolve-BinaryPath {
    param(
        [string]$Root,
        [string]$BinaryName
    )
    $candidates = @(
        (Join-Path $Root "target\release\$BinaryName"),
        (Join-Path $Root "rusty-kaspa\target\release\$BinaryName"),
        (Join-Path $Root "cpuminer\target\release\$BinaryName"),
        (Join-Path $Root "simply-kaspa-indexer\target\release\$BinaryName"),
        (Join-Path $Root "K-indexer\target\release\$BinaryName")
    )

    if ($BinaryName -eq "kasia-indexer.exe") {
        $candidates += (Join-Path $Root "kasia-indexer\target\release\indexer.exe")
    }

    foreach ($path in $candidates) {
        if (Test-Path -LiteralPath $path) {
            return $path
        }
    }
    return $null
}

function Copy-IfExists {
    param(
        [string]$Source,
        [string]$Destination
    )
    if (Test-Path -LiteralPath $Source) {
        Copy-Item -LiteralPath $Source -Destination $Destination -Recurse -Force
        return $true
    }
    return $false
}

function Get-NewestWriteTime {
    param(
        [string[]]$Paths
    )

    $newest = $null
    foreach ($path in $Paths) {
        if (-not (Test-Path -LiteralPath $path)) {
            continue
        }

        $item = Get-Item -LiteralPath $path
        if ($item.PSIsContainer) {
            $files = Get-ChildItem -LiteralPath $path -Recurse -File -ErrorAction SilentlyContinue
            foreach ($file in $files) {
                if (-not $newest -or $file.LastWriteTime -gt $newest) {
                    $newest = $file.LastWriteTime
                }
            }
        } else {
            if (-not $newest -or $item.LastWriteTime -gt $newest) {
                $newest = $item.LastWriteTime
            }
        }
    }

    return $newest
}

function Ensure-Explorer-Build {
    param([string]$Root)

    $explorerRoot = Join-Path $Root "kaspa-explorer-ng"
    if (-not (Test-Path -LiteralPath $explorerRoot)) {
        throw "Explorer source directory is missing: $explorerRoot"
    }

    $sourcePaths = @(
        (Join-Path $explorerRoot "package.json"),
        (Join-Path $explorerRoot "react-router.config.ts"),
        (Join-Path $explorerRoot "app"),
        (Join-Path $explorerRoot "public")
    )
    $sourceNewest = Get-NewestWriteTime -Paths $sourcePaths
    $repoBuildIndex = Join-Path $explorerRoot "build\client\index.html"
    $releaseBuildIndex = Join-Path $Root "target\release\kaspa-explorer-ng\build\client\index.html"
    $repoBuildTime = if (Test-Path -LiteralPath $repoBuildIndex) { (Get-Item -LiteralPath $repoBuildIndex).LastWriteTime } else { $null }
    $releaseBuildTime = if (Test-Path -LiteralPath $releaseBuildIndex) { (Get-Item -LiteralPath $releaseBuildIndex).LastWriteTime } else { $null }

    $needsBuild = (-not $repoBuildTime) -or (-not $releaseBuildTime) -or ($sourceNewest -and ($sourceNewest -gt $repoBuildTime))
    if (-not $needsBuild) {
        return
    }

    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
        throw "Explorer build is stale/missing and npm is not installed/available in PATH."
    }

    Write-Info "Explorer build is stale/missing. Running fallback build in kaspa-explorer-ng/ ..."
    Push-Location $explorerRoot
    try {
        if (Test-Path -LiteralPath "package-lock.json") {
            & npm ci --prefer-offline --no-audit --no-fund
            if ($LASTEXITCODE -ne 0) {
                & npm install --no-audit --no-fund
                if ($LASTEXITCODE -ne 0) {
                    throw "npm install failed in kaspa-explorer-ng/"
                }
            }
        } else {
            & npm install --no-audit --no-fund
            if ($LASTEXITCODE -ne 0) {
                throw "npm install failed in kaspa-explorer-ng/"
            }
        }

        & npm run build
        if ($LASTEXITCODE -ne 0) {
            throw "npm run build failed in kaspa-explorer-ng/"
        }
    } finally {
        Pop-Location
    }

    if (-not (Test-Path -LiteralPath $repoBuildIndex)) {
        throw "Explorer build did not produce kaspa-explorer-ng/build/client/index.html."
    }
}

function Ensure-Explorer-In-Package {
    param(
        [string]$Root,
        [string]$PackagePath
    )

    Ensure-Explorer-Build -Root $Root

    $packageExplorerRoot = Join-Path $PackagePath "kaspa-explorer-ng"
    $packageExplorerBuild = Join-Path $packageExplorerRoot "build"
    if (Test-Path -LiteralPath $packageExplorerBuild) {
        Remove-Item -LiteralPath $packageExplorerBuild -Recurse -Force
    }
    New-Item -ItemType Directory -Path $packageExplorerRoot -Force | Out-Null

    $releaseBuild = Join-Path $Root "target\release\kaspa-explorer-ng\build"
    $repoBuild = Join-Path $Root "kaspa-explorer-ng\build"
    $releaseIndex = Join-Path $releaseBuild "client\index.html"
    $repoIndex = Join-Path $repoBuild "client\index.html"

    $copyFrom = $null
    if ((Test-Path -LiteralPath $releaseIndex) -and (Test-Path -LiteralPath $repoIndex)) {
        $releaseTime = (Get-Item -LiteralPath $releaseIndex).LastWriteTime
        $repoTime = (Get-Item -LiteralPath $repoIndex).LastWriteTime
        $copyFrom = if ($repoTime -ge $releaseTime) { $repoBuild } else { $releaseBuild }
    } elseif (Test-Path -LiteralPath $repoIndex) {
        $copyFrom = $repoBuild
    } elseif (Test-Path -LiteralPath $releaseIndex) {
        $copyFrom = $releaseBuild
    }

    if ($copyFrom) {
        Copy-Item -LiteralPath $copyFrom -Destination $packageExplorerBuild -Recurse -Force
    }

    if (-not (Test-Path -LiteralPath (Join-Path $packageExplorerBuild "client\index.html"))) {
        throw "Explorer build not found. Run `npm install` and `npm run build` in `kaspa-explorer-ng`."
    }
}

function Build-PackageLayout {
    param(
        [string]$Root,
        [string]$OutDir
    )

    if (Test-Path -LiteralPath $OutDir) {
        Remove-Item -LiteralPath $OutDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $OutDir | Out-Null

    $requiredBins = @(
        "kaspa-ng.exe",
        "stratum-bridge.exe",
        "kaspa-miner.exe",
        "rothschild.exe",
        "simply-kaspa-indexer.exe",
        "K-webserver.exe",
        "K-transaction-processor.exe",
        "kasia-indexer.exe"
    )

    foreach ($bin in $requiredBins) {
        $src = Resolve-BinaryPath -Root $Root -BinaryName $bin
        if (-not $src) {
            throw "Missing packaged binary: $bin"
        }
        Copy-Item -LiteralPath $src -Destination (Join-Path $OutDir $bin) -Force
    }

    $explorerTarget = Join-Path $OutDir "kaspa-explorer-ng"
    if (Copy-IfExists -Source (Join-Path $Root "target\release\kaspa-explorer-ng") -Destination $explorerTarget) {
        # already copied
    } else {
        $explorerBuild = Join-Path $Root "kaspa-explorer-ng\build"
        if (-not (Test-Path -LiteralPath $explorerBuild)) {
            throw "Missing packaged directory: kaspa-explorer-ng"
        }
        New-Item -ItemType Directory -Path $explorerTarget -Force | Out-Null
        Copy-Item -LiteralPath $explorerBuild -Destination (Join-Path $explorerTarget "build") -Recurse -Force
    }

    foreach ($dir in @("kaspa-rest-server", "kaspa-socket-server")) {
        $src = Join-Path $Root $dir
        if (-not (Test-Path -LiteralPath $src)) {
            throw "Missing packaged directory: $dir"
        }
        Copy-Item -LiteralPath $src -Destination (Join-Path $OutDir $dir) -Recurse -Force
    }

    $kTarget = Join-Path $OutDir "K"
    if (-not (Copy-IfExists -Source (Join-Path $Root "target\release\K\dist") -Destination (Join-Path $kTarget "dist"))) {
        if (Test-Path -LiteralPath (Join-Path $Root "K\dist")) {
            New-Item -ItemType Directory -Path $kTarget -Force | Out-Null
            Copy-Item -LiteralPath (Join-Path $Root "K\dist") -Destination (Join-Path $kTarget "dist") -Recurse -Force
        }
    }

    $kasiaTarget = Join-Path $OutDir "Kasia"
    if (-not (Copy-IfExists -Source (Join-Path $Root "target\release\Kasia\dist") -Destination (Join-Path $kasiaTarget "dist"))) {
        if (Test-Path -LiteralPath (Join-Path $Root "Kasia\dist")) {
            New-Item -ItemType Directory -Path $kasiaTarget -Force | Out-Null
            Copy-Item -LiteralPath (Join-Path $Root "Kasia\dist") -Destination (Join-Path $kasiaTarget "dist") -Recurse -Force
        }
    }

    $kasVaultTarget = Join-Path $OutDir "KasVault"
    if (-not (Copy-IfExists -Source (Join-Path $Root "target\release\KasVault\build") -Destination (Join-Path $kasVaultTarget "build"))) {
        if (Test-Path -LiteralPath (Join-Path $Root "kasvault\build")) {
            New-Item -ItemType Directory -Path $kasVaultTarget -Force | Out-Null
            Copy-Item -LiteralPath (Join-Path $Root "kasvault\build") -Destination (Join-Path $kasVaultTarget "build") -Recurse -Force
        }
    }
}

function Ensure-K-Build {
    param([string]$Root)

    if (Test-Path -LiteralPath (Join-Path $Root "target\release\K\dist")) { return }
    if (Test-Path -LiteralPath (Join-Path $Root "K\dist")) { return }

    $kRoot = Join-Path $Root "K"
    if (-not (Test-Path -LiteralPath $kRoot)) {
        throw "K source directory is missing: $kRoot"
    }
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
        throw "K build not found and npm is not installed/available in PATH."
    }

    Write-Info "K build output missing. Running fallback build in K/ ..."
    Push-Location $kRoot
    try {
        if (Test-Path -LiteralPath "package-lock.json") {
            & npm ci --prefer-offline --no-audit --no-fund
            if ($LASTEXITCODE -ne 0) {
                & npm install --no-audit --no-fund
                if ($LASTEXITCODE -ne 0) {
                    throw "npm install failed in K/"
                }
            }
        } else {
            & npm install --no-audit --no-fund
            if ($LASTEXITCODE -ne 0) {
                throw "npm install failed in K/"
            }
        }

        & npm run build
        if ($LASTEXITCODE -ne 0) {
            throw "npm run build failed in K/"
        }
    } finally {
        Pop-Location
    }

    if (-not (Test-Path -LiteralPath (Join-Path $Root "K\dist"))) {
        throw "K build did not produce K/dist."
    }
}

function Ensure-K-In-Package {
    param(
        [string]$Root,
        [string]$PackagePath
    )

    $packageKDist = Join-Path $PackagePath "K\dist"
    if (Test-Path -LiteralPath $packageKDist) {
        return
    }

    Ensure-K-Build -Root $Root

    $kTarget = Join-Path $PackagePath "K"
    New-Item -ItemType Directory -Path $kTarget -Force | Out-Null

    if (Test-Path -LiteralPath (Join-Path $Root "target\release\K\dist")) {
        Copy-Item -LiteralPath (Join-Path $Root "target\release\K\dist") -Destination $packageKDist -Recurse -Force
    } elseif (Test-Path -LiteralPath (Join-Path $Root "K\dist")) {
        Copy-Item -LiteralPath (Join-Path $Root "K\dist") -Destination $packageKDist -Recurse -Force
    }

    if (-not (Test-Path -LiteralPath $packageKDist)) {
        throw "K build not found. Run `npm install` and `npm run build` in `K`."
    }
}

function Ensure-Kasia-Build {
    param([string]$Root)

    if (Test-Path -LiteralPath (Join-Path $Root "target\release\Kasia\dist")) { return }
    if (Test-Path -LiteralPath (Join-Path $Root "Kasia\dist")) { return }

    $kasiaRoot = Join-Path $Root "Kasia"
    if (-not (Test-Path -LiteralPath $kasiaRoot)) {
        throw "Kasia source directory is missing: $kasiaRoot"
    }
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
        throw "Kasia build not found and npm is not installed/available in PATH."
    }

    function Ensure-Kasia-WasmPackage {
        param([string]$WorkspaceRoot, [string]$KasiaDir)
        $kasiaWasm = Join-Path $KasiaDir "wasm"
        if (Test-Path -LiteralPath (Join-Path $kasiaWasm "package.json")) {
            return
        }

        $sdkCandidates = @(
            (Join-Path $WorkspaceRoot "K\src\kaspa-wasm32-sdk\web\kaspa"),
            (Join-Path $WorkspaceRoot "rusty-kaspa\wasm\npm\kaspa")
        )
        foreach ($candidate in $sdkCandidates) {
            if (Test-Path -LiteralPath (Join-Path $candidate "package.json")) {
                Write-Info "Populating Kasia/wasm from $candidate"
                if (Test-Path -LiteralPath $kasiaWasm) {
                    Remove-Item -LiteralPath $kasiaWasm -Recurse -Force
                }
                Copy-Item -LiteralPath $candidate -Destination $kasiaWasm -Recurse -Force
                return
            }
        }
        throw "Missing Kasia wasm SDK package (Kasia/wasm)."
    }

    function Ensure-ClangAvailable {
        if (Get-Command clang -ErrorAction SilentlyContinue) {
            return
        }

        Write-Info "clang not found. Attempting LLVM install via winget..."
        if (Get-Command winget -ErrorAction SilentlyContinue) {
            try {
                & winget install -e --id LLVM.LLVM --accept-source-agreements --accept-package-agreements --silent
            } catch {
                Write-Info "winget LLVM install failed: $($_.Exception.Message)"
            }
        } elseif (Get-Command choco -ErrorAction SilentlyContinue) {
            try {
                & choco install llvm -y
            } catch {
                Write-Info "choco LLVM install failed: $($_.Exception.Message)"
            }
        }

        $llvmBin = "C:\Program Files\LLVM\bin"
        if ((Test-Path -LiteralPath $llvmBin) -and -not (($env:PATH -split ';') -contains $llvmBin)) {
            $env:PATH = "$llvmBin;$env:PATH"
        }

        if (-not (Get-Command clang -ErrorAction SilentlyContinue)) {
            throw "clang is required for Kasia wasm build (cipher-wasm). Install LLVM and re-run MSI build."
        }
    }

    Write-Info "Kasia build output missing. Running fallback build in Kasia/ ..."
    Ensure-Kasia-WasmPackage -WorkspaceRoot $Root -KasiaDir $kasiaRoot

    Push-Location $kasiaRoot
    try {
        if (-not (Test-Path -LiteralPath "cipher-wasm\package.json")) {
            Ensure-ClangAvailable
            & npm run wasm:build
            if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath "cipher-wasm\package.json")) {
                throw "Failed to build Kasia cipher-wasm. Ensure LLVM/clang and wasm-pack are installed."
            }
        }

        if (Test-Path -LiteralPath "package-lock.json") {
            & npm ci --prefer-offline --no-audit --no-fund
            if ($LASTEXITCODE -ne 0) {
                & npm install --no-audit --no-fund
                if ($LASTEXITCODE -ne 0) {
                    throw "npm install failed in Kasia/"
                }
            }
        } else {
            & npm install --no-audit --no-fund
            if ($LASTEXITCODE -ne 0) {
                throw "npm install failed in Kasia/"
            }
        }

        & npm run build:production
        if ($LASTEXITCODE -ne 0) {
            & npm exec vite build
            if ($LASTEXITCODE -ne 0) {
                throw "Kasia build failed (build:production and vite build)."
            }
        }
    } finally {
        Pop-Location
    }

    if (-not (Test-Path -LiteralPath (Join-Path $Root "Kasia\dist"))) {
        throw "Kasia build did not produce Kasia/dist."
    }
}

function Ensure-Kasia-In-Package {
    param(
        [string]$Root,
        [string]$PackagePath
    )

    $packageKasiaDist = Join-Path $PackagePath "Kasia\dist"
    if (Test-Path -LiteralPath $packageKasiaDist) {
        return
    }

    Ensure-Kasia-Build -Root $Root

    $kasiaTarget = Join-Path $PackagePath "Kasia"
    New-Item -ItemType Directory -Path $kasiaTarget -Force | Out-Null

    if (Test-Path -LiteralPath (Join-Path $Root "target\release\Kasia\dist")) {
        Copy-Item -LiteralPath (Join-Path $Root "target\release\Kasia\dist") -Destination $packageKasiaDist -Recurse -Force
    } elseif (Test-Path -LiteralPath (Join-Path $Root "Kasia\dist")) {
        Copy-Item -LiteralPath (Join-Path $Root "Kasia\dist") -Destination $packageKasiaDist -Recurse -Force
    }

    if (-not (Test-Path -LiteralPath $packageKasiaDist)) {
        throw "Kasia build not found. Run `npm install` and `npm run build:production` in `Kasia`."
    }
}

function Ensure-KasVault-Build {
    param([string]$Root)

    if (Test-Path -LiteralPath (Join-Path $Root "target\release\KasVault\build\index.html")) { return }
    if (Test-Path -LiteralPath (Join-Path $Root "kasvault\build\index.html")) { return }

    $kasVaultRoot = Join-Path $Root "kasvault"
    if (-not (Test-Path -LiteralPath $kasVaultRoot)) {
        if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
            throw "KasVault source directory is missing and git is unavailable: $kasVaultRoot"
        }
        Write-Info "KasVault source missing. Cloning repository..."
        Push-Location $Root
        try {
            & git clone --depth 1 https://github.com/coderofstuff/kasvault.git kasvault
            if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $kasVaultRoot)) {
                throw "Failed to clone KasVault repository."
            }
        } finally {
            Pop-Location
        }
    }
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
        throw "KasVault build not found and npm is not installed/available in PATH."
    }

    Write-Info "KasVault build output missing. Running fallback build in kasvault/ ..."
    Push-Location $kasVaultRoot
    try {
        if (Test-Path -LiteralPath "package-lock.json") {
            & npm ci --prefer-offline --no-audit --no-fund
            if ($LASTEXITCODE -ne 0) {
                & npm install --no-audit --no-fund
                if ($LASTEXITCODE -ne 0) {
                    throw "npm install failed in kasvault/"
                }
            }
        } else {
            & npm install --no-audit --no-fund
            if ($LASTEXITCODE -ne 0) {
                throw "npm install failed in kasvault/"
            }
        }

        $previousCi = $env:CI
        $previousDisableEslint = $env:DISABLE_ESLINT_PLUGIN
        $env:CI = "false"
        $env:DISABLE_ESLINT_PLUGIN = "true"
        & npm run build
        if ($null -ne $previousCi) {
            $env:CI = $previousCi
        } else {
            Remove-Item Env:CI -ErrorAction SilentlyContinue
        }
        if ($null -ne $previousDisableEslint) {
            $env:DISABLE_ESLINT_PLUGIN = $previousDisableEslint
        } else {
            Remove-Item Env:DISABLE_ESLINT_PLUGIN -ErrorAction SilentlyContinue
        }
        if ($LASTEXITCODE -ne 0) {
            throw "npm run build failed in kasvault/"
        }
    } finally {
        Pop-Location
    }

    if (-not (Test-Path -LiteralPath (Join-Path $Root "kasvault\build\index.html"))) {
        throw "KasVault build did not produce kasvault/build."
    }
}

function Ensure-KasVault-In-Package {
    param(
        [string]$Root,
        [string]$PackagePath
    )

    $packageKasVaultBuild = Join-Path $PackagePath "KasVault\build"
    if (Test-Path -LiteralPath (Join-Path $packageKasVaultBuild "index.html")) {
        return
    }

    Ensure-KasVault-Build -Root $Root

    $kasVaultTarget = Join-Path $PackagePath "KasVault"
    New-Item -ItemType Directory -Path $kasVaultTarget -Force | Out-Null
    if (Test-Path -LiteralPath $packageKasVaultBuild) {
        Remove-Item -LiteralPath $packageKasVaultBuild -Recurse -Force
    }
    New-Item -ItemType Directory -Path $packageKasVaultBuild -Force | Out-Null

    if (Test-Path -LiteralPath (Join-Path $Root "target\release\KasVault\build\index.html")) {
        Copy-Item -Path (Join-Path $Root "target\release\KasVault\build\*") -Destination $packageKasVaultBuild -Recurse -Force
    } elseif (Test-Path -LiteralPath (Join-Path $Root "kasvault\build\index.html")) {
        Copy-Item -Path (Join-Path $Root "kasvault\build\*") -Destination $packageKasVaultBuild -Recurse -Force
    }

    if (-not (Test-Path -LiteralPath (Join-Path $packageKasVaultBuild "index.html"))) {
        throw 'KasVault build not found. Run `npm install` and `npm run build` in `kasvault`.'
    }
}

function Build-Msi {
    param(
        [string]$Root,
        [string]$SourceDir,
        [string]$RawVersion
    )

    Ensure-Wix

    $msiVersion = Normalize-MsiVersion -RawVersion $RawVersion
    $outputFile = Join-Path (Split-Path -Parent $SourceDir) ("kaspa-ng-windows-x64-{0}.msi" -f $RawVersion)
    $wxsFile = Join-Path $env:TEMP "kaspa-ng-installer.wxs"
    $sourceRoot = [System.IO.Path]::GetFullPath($SourceDir)

    function Get-HashId([string]$Prefix, [string]$InputText) {
        $sha = [System.Security.Cryptography.SHA1]::Create()
        try {
            $bytes = [System.Text.Encoding]::UTF8.GetBytes($InputText)
            $hash = $sha.ComputeHash($bytes)
            $hex = [System.BitConverter]::ToString($hash).Replace("-", "").ToLowerInvariant()
            return "${Prefix}_$($hex.Substring(0, 16))"
        } finally {
            $sha.Dispose()
        }
    }

    $files = Get-ChildItem -LiteralPath $sourceRoot -Recurse -File
    if (-not $files) {
        throw "Package directory is empty: $sourceRoot"
    }

    $dirComponents = New-Object 'System.Collections.Generic.Dictionary[string,object]'
    $dirComponents[""] = [ordered]@{
        Name = ""
        Id = "INSTALLFOLDER"
        Children = New-Object 'System.Collections.Generic.Dictionary[string,object]'
        Files = New-Object 'System.Collections.Generic.List[object]'
    }

    foreach ($file in $files) {
        $relative = $file.FullName.Substring($sourceRoot.Length).TrimStart('\', '/')
        $parts = $relative -split '[\\/]'
        $dirParts = @()
        if ($parts.Length -gt 1) {
            $dirParts = $parts[0..($parts.Length - 2)]
        }

        $currentRel = ""
        $currentNode = $dirComponents[""]
        foreach ($part in $dirParts) {
            $nextRel = if ([string]::IsNullOrEmpty($currentRel)) { $part } else { "$currentRel\$part" }
            if (-not $dirComponents.ContainsKey($nextRel)) {
                $dirNode = [ordered]@{
                    Name = $part
                    Id = (Get-HashId "DIR" $nextRel)
                    Children = New-Object 'System.Collections.Generic.Dictionary[string,object]'
                    Files = New-Object 'System.Collections.Generic.List[object]'
                }
                $dirComponents[$nextRel] = $dirNode
                $currentNode.Children[$nextRel] = $dirNode
            }
            $currentNode = $dirComponents[$nextRel]
            $currentRel = $nextRel
        }

        $fileRelKey = $relative.Replace('/', '\')
        $currentNode.Files.Add([ordered]@{
            Relative = $fileRelKey
            Source = $file.FullName
            ComponentId = (Get-HashId "CMP" $fileRelKey)
            FileId = (Get-HashId "FIL" $fileRelKey)
        })
    }

    function Escape-Xml([string]$Text) {
        return [System.Security.SecurityElement]::Escape($Text)
    }

    function Write-DirectoryNode([System.Text.StringBuilder]$Builder, $Node, [int]$Indent) {
        $pad = "  " * $Indent
        if ($Node.Id -ne "INSTALLFOLDER") {
            $nameEscaped = Escape-Xml $Node.Name
            [void]$Builder.AppendLine("$pad<Directory Id=""$($Node.Id)"" Name=""$nameEscaped"">")
        }

        $innerIndent = if ($Node.Id -eq "INSTALLFOLDER") { $Indent } else { $Indent + 1 }
        $innerPad = "  " * $innerIndent

        foreach ($fileEntry in $Node.Files) {
            $src = Escape-Xml $fileEntry.Source
            [void]$Builder.AppendLine("$innerPad<Component Id=""$($fileEntry.ComponentId)"" Guid=""*"">")
            [void]$Builder.AppendLine("$innerPad  <File Id=""$($fileEntry.FileId)"" Source=""$src"" KeyPath=""yes"" />")
            [void]$Builder.AppendLine("$innerPad</Component>")
        }

        foreach ($child in $Node.Children.Values) {
            Write-DirectoryNode -Builder $Builder -Node $child -Indent $innerIndent
        }

        if ($Node.Id -ne "INSTALLFOLDER") {
            [void]$Builder.AppendLine("$pad</Directory>")
        }
    }

    $builder = New-Object System.Text.StringBuilder
    [void]$builder.AppendLine('<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">')
    [void]$builder.AppendLine("  <Package")
    [void]$builder.AppendLine('    Name="Kaspa NG"')
    [void]$builder.AppendLine('    Manufacturer="ASPECTRON Inc."')
    [void]$builder.AppendLine("    Version=""$msiVersion""")
    [void]$builder.AppendLine('    UpgradeCode="5FC5A070-1E87-4D30-8FF7-1F16D6FE8AF4"')
    [void]$builder.AppendLine('    Language="1033"')
    [void]$builder.AppendLine('    Scope="perMachine">')
    [void]$builder.AppendLine('    <SummaryInformation Description="Kaspa NG desktop application" />')
    [void]$builder.AppendLine('    <MajorUpgrade DowngradeErrorMessage="A newer version of Kaspa NG is already installed." />')
    [void]$builder.AppendLine('    <MediaTemplate EmbedCab="yes" />')
    [void]$builder.AppendLine('    <StandardDirectory Id="ProgramFiles64Folder">')
    [void]$builder.AppendLine('      <Directory Id="INSTALLFOLDER" Name="Kaspa NG">')

    Write-DirectoryNode -Builder $builder -Node $dirComponents[""] -Indent 4

    [void]$builder.AppendLine('      </Directory>')
    [void]$builder.AppendLine('    </StandardDirectory>')
    [void]$builder.AppendLine('    <Feature Id="MainFeature" Title="Kaspa NG" Level="1">')
    [void]$builder.AppendLine('      <ComponentGroupRef Id="AppFiles" />')
    [void]$builder.AppendLine('    </Feature>')
    [void]$builder.AppendLine('  </Package>')
    [void]$builder.AppendLine('  <Fragment>')
    [void]$builder.AppendLine('    <ComponentGroup Id="AppFiles">')
    foreach ($file in $files) {
        $relative = $file.FullName.Substring($sourceRoot.Length).TrimStart('\', '/').Replace('/', '\')
        $componentId = Get-HashId "CMP" $relative
        [void]$builder.AppendLine("      <ComponentRef Id=""$componentId"" />")
    }
    [void]$builder.AppendLine('    </ComponentGroup>')
    [void]$builder.AppendLine('  </Fragment>')
    [void]$builder.AppendLine('</Wix>')

    [System.IO.File]::WriteAllText($wxsFile, $builder.ToString(), [System.Text.Encoding]::UTF8)

    & wix build $wxsFile -arch x64 -o $outputFile
    if ($LASTEXITCODE -ne 0) {
        throw "WiX build failed with exit code $LASTEXITCODE"
    }
    return $outputFile
}

$resolvedRoot = Get-RepoRoot
Write-Info "Repo root: $resolvedRoot"

$resolvedVersion = if ($Version) { $Version } else { Get-WorkspaceVersion -Root $resolvedRoot }
Write-Info "Version: $resolvedVersion"

if (-not $PackageDir) {
    $PackageDir = Join-Path $resolvedRoot ("kaspa-ng-{0}-windows-x64" -f $resolvedVersion)
}
$resolvedPackageDir = [System.IO.Path]::GetFullPath($PackageDir)
Write-Info "Package dir: $resolvedPackageDir"
$reuseExistingPackage = $PSBoundParameters.ContainsKey("PackageDir") -and (Test-Path -LiteralPath $resolvedPackageDir)

if (-not $SkipBuild.IsPresent) {
    Write-Info "Running cargo build --release"
    $env:KASIA_WASM_AUTO_FETCH = "0"
    Push-Location $resolvedRoot
    try {
        & cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --release failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

if ($reuseExistingPackage) {
    Write-Info "Using existing package directory as MSI input."
} else {
    Ensure-K-Build -Root $resolvedRoot
    Ensure-Kasia-Build -Root $resolvedRoot
    Ensure-KasVault-Build -Root $resolvedRoot
    Build-PackageLayout -Root $resolvedRoot -OutDir $resolvedPackageDir
}

Ensure-K-In-Package -Root $resolvedRoot -PackagePath $resolvedPackageDir
Ensure-Kasia-In-Package -Root $resolvedRoot -PackagePath $resolvedPackageDir
Ensure-KasVault-In-Package -Root $resolvedRoot -PackagePath $resolvedPackageDir
Ensure-Explorer-In-Package -Root $resolvedRoot -PackagePath $resolvedPackageDir

$msiFile = Build-Msi -Root $resolvedRoot -SourceDir $resolvedPackageDir -RawVersion $resolvedVersion
Write-Info "MSI package created: $msiFile"
