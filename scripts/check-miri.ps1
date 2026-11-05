param(
    [string]$Toolchain = 'nightly',
    [ValidateSet('Stacked', 'Tree', 'Both')]
    [string]$BorrowModel = 'Both',
    [uint32]$Seed = 0
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskLogDir = Join-Path $taskRoot "target/miri-audit/seed-$Seed"
$taskOldMiriFlags = [Environment]::GetEnvironmentVariable('MIRIFLAGS')
$taskResults = @()
$taskModels = if ($BorrowModel -eq 'Both') { @('Stacked', 'Tree') } else { @($BorrowModel) }
$taskGroups = @(
    @{ Name = 'task'; Args = @('--lib', 'task::') },
    @{ Name = 'host-resources'; Args = @('--test', 'host_resources') },
    @{ Name = 'request-routing'; Args = @('--test', 'request_routing') },
    @{ Name = 'restart'; Args = @('--test', 'restart') },
    @{ Name = 'host-commit'; Args = @('--test', 'host_commit') },
    @{ Name = 'managed-activities'; Args = @('--test', 'managed_activities') },
    @{ Name = 'async-services'; Args = @('--test', 'async_services') },
    @{ Name = 'threaded-services'; Args = @('--test', 'threaded_services') },
    @{ Name = 'service-contracts'; Args = @('--test', 'service_contracts') }
)

Push-Location $taskRoot
try {
    New-Item -ItemType Directory -Path $taskLogDir -Force | Out-Null
    $taskRustVersion = & rustc "+$Toolchain" -Vv
    if ($LASTEXITCODE -ne 0) { throw "Rust toolchain '$Toolchain' is not available." }
    $taskSourceFiles = @(
        'Cargo.toml', 'flow_reload/Cargo.toml', 'interface/Cargo.toml', 'hot_macro/Cargo.toml',
        'interface/src/lib.rs', 'hot_macro/src/lib.rs', 'flow_reload/src/lib.rs',
        'flow_reload/src/internal.rs', 'flow_reload/src/generation.rs', 'flow_reload/src/native.rs',
        'flow_reload/src/task/future.rs', 'flow_reload/src/task/waker.rs', 'flow_reload/src/task/mod.rs',
        'flow_reload/src/resources.rs', 'flow_reload/src/request.rs', 'flow_reload/src/routing.rs',
        'interface/src/services.rs', 'hot_macro/src/handler.rs',
        'flow_reload/src/routing/control.rs', 'flow_reload/src/routing/limits.rs',
        'flow_reload/src/routing/restart.rs', 'flow_reload/tests/restart.rs',
        'flow_reload/src/retention.rs', 'flow_reload/src/cache.rs',
        'flow_reload/src/cache/session.rs', 'flow_reload/src/cache/permissions.rs',
        'flow_reload/tests/host_commit.rs',
        'flow_reload/src/activity.rs', 'flow_reload/tests/managed_activities.rs',
        'flow_reload/src/async_service.rs', 'flow_reload/src/resources/async_ops.rs',
        'flow_reload/src/thread_service.rs', 'flow_reload/tests/async_services.rs',
        'flow_reload/tests/threaded_services.rs',
        'flow_reload/src/environment.rs', 'flow_reload/tests/service_contracts.rs',
        'flow_reload/tests/host_resources.rs', 'flow_reload/tests/request_routing.rs',
        'Cargo.lock'
    )
    $taskHashes = foreach ($taskFile in $taskSourceFiles) {
        if (Test-Path -LiteralPath $taskFile) {
            @{ path = $taskFile; sha256 = (Get-FileHash -LiteralPath $taskFile -Algorithm SHA256).Hash }
        }
    }
    @{
        toolchain = $Toolchain
        rustc = $taskRustVersion
        target = 'x86_64-unknown-linux-gnu'
        seed = $Seed
        source_files = @($taskHashes)
    } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $taskLogDir 'environment.json')

    foreach ($taskModel in $taskModels) {
        $env:MIRIFLAGS = "-Zmiri-backtrace=full -Zmiri-seed=$Seed"
        if ($taskModel -eq 'Tree') { $env:MIRIFLAGS += ' -Zmiri-tree-borrows' }
        foreach ($taskGroup in $taskGroups) {
            $taskLogName = "$($taskModel.ToLowerInvariant())-$($taskGroup.Name).log"
            $taskArgs = @(
                "+$Toolchain", 'miri', 'test', '-p', 'flow_reload',
                '--no-default-features', '--locked', '--target', 'x86_64-unknown-linux-gnu'
            ) + $taskGroup.Args
            Write-Host "Running $taskModel / $($taskGroup.Name) (seed $Seed)"
            & cargo @taskArgs 2>&1 | Tee-Object -FilePath (Join-Path $taskLogDir $taskLogName)
            $taskResults += [pscustomobject]@{
                model = $taskModel
                group = $taskGroup.Name
                exit_code = $LASTEXITCODE
                log = $taskLogName
            }
        }
    }
    $taskResults | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $taskLogDir 'results.json')
    $taskResults | Format-Table -AutoSize
    $taskFailed = @($taskResults | Where-Object { $_.exit_code -ne 0 }).Count
}
finally {
    [Environment]::SetEnvironmentVariable('MIRIFLAGS', $taskOldMiriFlags)
    Pop-Location
}

if ($taskFailed -gt 0) { exit 1 }
