#Requires -Version 5.1
# PowerShell equivalent of run_suas.sh, since PowerShell cannot execute .sh files.
#
# The bash version uses `exec`, so its environment dies with the process. A
# PowerShell script shares the caller's process, so env vars and the working
# directory are snapshotted and restored to keep them scoped to this run.
$ErrorActionPreference = 'Stop'

$env_file = Join-Path $PSScriptRoot 'config\suas.env'
$saved = @{}
$status = 1

Push-Location $PSScriptRoot
try {
    foreach ($line in Get-Content -Path $env_file) {
        $trimmed = $line.Trim()
        if ($trimmed -eq '' -or $trimmed.StartsWith('#')) { continue }

        $pair = $trimmed.Split('=', 2)
        if ($pair.Count -ne 2) { throw "Malformed line in ${env_file}: $line" }

        $key = $pair[0].Trim()
        $saved[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
        [Environment]::SetEnvironmentVariable($key, $pair[1].Trim(), 'Process')
    }

    & cargo run @args
    $status = $LASTEXITCODE
}
finally {
    foreach ($key in $saved.Keys) {
        [Environment]::SetEnvironmentVariable($key, $saved[$key], 'Process')
    }
    Pop-Location
}

exit $status
