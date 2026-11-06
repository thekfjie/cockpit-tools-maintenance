# Tauri embeds Common Controls v6 into app binaries, but not lib-test EXEs.
# Only the CI test artifact is changed: https://github.com/tauri-apps/tauri/issues/13419
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$buildArgs = @('test', '-p', 'cockpit-tools', '--lib', '--profile', 'ci', '--locked',
    '--features', 'tauri/custom-protocol', '--no-run', '--message-format=json-render-diagnostics')
$messages = & cargo @buildArgs
$buildExitCode = $LASTEXITCODE
$artifacts = @()
foreach ($line in $messages) {
    $message = $line | ConvertFrom-Json
    if ($message.reason -eq 'compiler-message') {
        Write-Host $message.message.rendered
    }
    if ($message.reason -eq 'compiler-artifact' -and
        $message.target.name -eq 'antigravity_cockpit_tools_lib' -and
        $message.profile.test -and $message.executable) {
        $artifacts += $message.executable
    }
}
if ($buildExitCode -ne 0) { throw "Rust test compilation failed: $buildExitCode" }
if ($artifacts.Count -ne 1) { throw "Expected one lib-test executable, found $($artifacts.Count)" }

$testExe = [IO.Path]::GetFullPath($artifacts[0])
$targetRoot = [IO.Path]::GetFullPath((Join-Path (Get-Location) 'target')) + [IO.Path]::DirectorySeparatorChar
if (!$testExe.StartsWith($targetRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Test executable is outside this checkout target directory'
}
$programFiles86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
$kits = Join-Path $programFiles86 'Windows Kits\10\bin'
$mt = Get-ChildItem -Path "$kits\*\x64\mt.exe" |
    Sort-Object FullName -Descending | Select-Object -First 1
if (!$mt) { throw 'Windows SDK mt.exe not found' }
$manifestPath = Join-Path $env:RUNNER_TEMP 'cockpit-lib-test.manifest'
$extractedPath = Join-Path $env:RUNNER_TEMP 'cockpit-lib-test-extracted.manifest'
$manifest = @'
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0" processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
</assembly>
'@
try {
    [IO.File]::WriteAllText($manifestPath, $manifest, [Text.UTF8Encoding]::new($false))
    & $mt.FullName -nologo -manifest $manifestPath "-outputresource:$testExe;#1"
    if ($LASTEXITCODE -ne 0) { throw 'Embedding test manifest failed' }
    & $mt.FullName -nologo "-inputresource:$testExe;#1" "-out:$extractedPath"
    if ($LASTEXITCODE -ne 0) { throw 'Reading embedded test manifest failed' }
    if (!(Get-Content -LiteralPath $extractedPath -Raw).Contains('Microsoft.Windows.Common-Controls')) {
        throw 'Common Controls v6 manifest was not embedded'
    }

    $filters = @('account_project_switch', 'local_projects',
        'provider_gateway_port_probe', 'occupied_persisted_gateway_port_is_released_before_start')
    foreach ($filter in $filters) {
        $listed = & $testExe $filter --list
        if ($LASTEXITCODE -ne 0) { throw "Listing Rust regressions failed: $filter" }
        if (@($listed | Where-Object { $_ -match ': test$' }).Count -eq 0) {
            throw "No Rust regressions matched filter: $filter"
        }
        & $testExe $filter --test-threads=1 --nocapture 2>&1 |
            Tee-Object -FilePath "$filter-tests.log"
        if ($LASTEXITCODE -ne 0) { throw "Rust regressions failed: $filter ($LASTEXITCODE)" }
    }
} finally {
    foreach ($path in @($manifestPath, $extractedPath)) {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
    }
}
