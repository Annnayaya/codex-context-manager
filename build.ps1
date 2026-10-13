param(
    [switch]$SkipChecks,
    [switch]$UseMirror
)

$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$previousPath = $env:Path
$previousLocation = Get-Location
$previousEncodedRustFlags = $env:CARGO_ENCODED_RUSTFLAGS

try {
    Set-Location -LiteralPath $projectRoot
    $toolchain = rustup show active-toolchain
    if ($LASTEXITCODE -ne 0) { throw 'Rust toolchain is unavailable.' }
    if ($toolchain -match 'windows-gnu' -and -not (Get-Command gcc -ErrorAction SilentlyContinue)) {
        $mingwCandidates = @($env:MINGW_BIN, 'C:/msys64/mingw64/bin', 'C:/msys64/ucrt64/bin')
        $mingwBin = $mingwCandidates | Where-Object { $_ -and (Test-Path -LiteralPath (Join-Path $_ 'gcc.exe')) } | Select-Object -First 1
        if (-not $mingwBin) { throw 'Set MINGW_BIN to your MinGW bin folder, or use a Windows MSVC Rust toolchain.' }
        $env:Path = "$mingwBin;$env:Path"
    }

    # Avoid embedding local user, workspace and toolchain paths in distributable binaries.
    # Use encoded flags so paths containing spaces remain a single compiler argument.
    $privacyRustFlags = [System.Collections.Generic.List[string]]::new()
    if ($previousEncodedRustFlags) {
        foreach ($flag in $previousEncodedRustFlags.Split([char]31)) { $privacyRustFlags.Add($flag) }
    } elseif ($env:RUSTFLAGS) {
        foreach ($flag in ($env:RUSTFLAGS -split '\s+' | Where-Object { $_ })) { $privacyRustFlags.Add($flag) }
    }
    $profileRoot = [Environment]::GetFolderPath('UserProfile')
    $cargoRoot = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $profileRoot '.cargo' }
    $rustupRoot = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $profileRoot '.rustup' }
    $privacyMappings = @(
        @{ Source = $profileRoot; Replacement = '/user' },
        @{ Source = $cargoRoot; Replacement = '/cargo' },
        @{ Source = $rustupRoot; Replacement = '/rustup' },
        @{ Source = $projectRoot; Replacement = '/codex-context-manager' }
    )
    foreach ($mapping in $privacyMappings) {
        if (-not $mapping.Source) { continue }
        $mappingPath = [System.IO.Path]::GetFullPath($mapping.Source).TrimEnd([char]'\', [char]'/')
        foreach ($spelling in @($mappingPath, $mappingPath.Replace('\', '/')) | Select-Object -Unique) {
            $privacyRustFlags.Add("--remap-path-prefix=$spelling=$($mapping.Replacement)")
        }
    }
    $env:CARGO_ENCODED_RUSTFLAGS = [string]::Join([char]31, $privacyRustFlags)

    $cargoPrefix = @()
    if ($UseMirror) {
        $cargoPrefix = @('--config', 'source.crates-io.replace-with="rsproxy"', '--config', 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"')
    }
    if (-not $SkipChecks) {
        & cargo @cargoPrefix fmt --all -- --check
        if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed.' }
        & cargo @cargoPrefix test --locked
        if ($LASTEXITCODE -ne 0) { throw 'Tests failed.' }
        & cargo clippy @cargoPrefix --locked --all-targets -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw 'Clippy check failed.' }
    }
    & cargo @cargoPrefix build --locked --release
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }

    $outputDirectory = Join-Path $projectRoot 'dist'
    New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
    $outputExecutable = Join-Path $outputDirectory 'Codex上下文管理.exe'
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target/release/codex-context-manager.exe') -Destination $outputExecutable -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination (Join-Path $outputDirectory '使用说明.md') -Force
    $releaseFiles = @($outputExecutable, (Join-Path $outputDirectory '使用说明.md'))

    if ($toolchain -match 'windows-gnu') {
        $gcc = Get-Command gcc -ErrorAction SilentlyContinue
        if ($gcc) {
            $gccDirectory = Split-Path -Parent $gcc.Source
            $objdumpPath = Join-Path $gccDirectory 'objdump.exe'
            # Some MinGW tools cannot open a Unicode filename; inspect the original ASCII name.
            $imports = if (Test-Path -LiteralPath $objdumpPath) {
                & $objdumpPath -p (Join-Path $projectRoot 'target/release/codex-context-manager.exe') | Out-String
            } else { $null }
            foreach ($library in @('libgcc_s_seh-1.dll', 'libwinpthread-1.dll')) {
                $libraryPath = Join-Path $gccDirectory $library
                if ((-not $imports -or $imports -match [regex]::Escape($library)) -and (Test-Path -LiteralPath $libraryPath)) {
                    Copy-Item -LiteralPath $libraryPath -Destination (Join-Path $outputDirectory $library) -Force
                    $releaseFiles += Join-Path $outputDirectory $library
                }
            }
        }
    }
    # Package only release files, never unrelated files left in the output directory.
    Compress-Archive -LiteralPath $releaseFiles -DestinationPath (Join-Path $projectRoot 'Codex上下文管理-Windows.zip') -Force
    Write-Output "Built: $outputExecutable"
} finally {
    $env:Path = $previousPath
    $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedRustFlags
    Set-Location -LiteralPath $previousLocation.Path
}
