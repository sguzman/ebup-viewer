[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$scriptPath = Join-Path $PSScriptRoot 'codex-goal-notify.ps1'
$root = Join-Path ([IO.Path]::GetTempPath()) "lanternleaf-goal-notify-$([Guid]::NewGuid())"
New-Item -ItemType Directory -Path (Join-Path $root 'docs\work\done'), (Join-Path $root 'docs\work\blocked'), (Join-Path $root '.git\lanternleaf-goal-state') -Force | Out-Null

function Invoke-Probe([string] $stateFile, [string] $expected, [int] $expectedExit = 0) {
    $output = & pwsh -NoProfile -File $scriptPath -GoalId 0005 -RepoRoot $root -TerminalStateFile $stateFile -PollSeconds 1 -MaxPolls 1 -TestMode 2>&1
    if ($LASTEXITCODE -ne $expectedExit) { throw "Expected exit $expectedExit, got ${LASTEXITCODE}: $($output -join ' ')" }
    if ($expected -and -not (($output -join "`n") -match [regex]::Escape($expected))) { throw "Expected '$expected': $($output -join ' ')" }
}

$sentinel = Join-Path $root '.git\lanternleaf-goal-state\0005.terminal'
Invoke-Probe $sentinel 'WATCH_TIMEOUT' 2
Set-Content -LiteralPath (Join-Path $root 'docs\work\done\0005.md') -Value '# done'
Invoke-Probe $sentinel 'TEST_NOTIFICATION|done'
Invoke-Probe $sentinel 'ALREADY_ACKNOWLEDGED'

Remove-Item -LiteralPath (Join-Path $root 'docs\work\done\0005.md') -Force
$blockedSentinel = Join-Path $root '.git\lanternleaf-goal-state\0005-blocked.terminal'
Set-Content -LiteralPath $blockedSentinel -Value 'blocked'
Invoke-Probe $blockedSentinel 'TEST_NOTIFICATION|blocked'

$malformed = Join-Path $root '.git\lanternleaf-goal-state\0005-malformed.terminal'
Set-Content -LiteralPath $malformed -Value 'not-terminal'
Invoke-Probe $malformed 'WATCH_TIMEOUT' 2

Write-Output 'codex-goal-notify tests passed'
Remove-Item -LiteralPath $root -Recurse -Force
