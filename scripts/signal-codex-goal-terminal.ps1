[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $GoalId,
    [Parameter(Mandatory = $true)] [string] $RepoRoot,
    [Parameter(Mandatory = $true)] [ValidateSet('done', 'blocked')] [string] $State,
    [int] $AckWaitSeconds = 15
)

$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath($RepoRoot)
$stateRoot = Join-Path $repo '.git\lanternleaf-goal-state'
$terminal = Join-Path $stateRoot "$GoalId.terminal"
$ack = "$terminal.ack"

New-Item -ItemType Directory -Path $stateRoot -Force | Out-Null

if (Test-Path -LiteralPath $terminal) {
    $existing = (Get-Content -LiteralPath $terminal -Raw).Trim().ToLowerInvariant()
    if ($existing -and $existing -ne $State) {
        throw "Goal $GoalId terminal state is already '$existing'; refusing to change it to '$State'."
    }
}

$temp = "$terminal.tmp.$PID"
Set-Content -LiteralPath $temp -Value $State -Encoding utf8
Move-Item -LiteralPath $temp -Destination $terminal -Force
Write-Output "GOAL_TERMINAL_SIGNALLED|$GoalId|$State"

$deadline = [DateTime]::UtcNow.AddSeconds([Math]::Max(0, $AckWaitSeconds))
while ([DateTime]::UtcNow -lt $deadline) {
    if (Test-Path -LiteralPath $ack) {
        $ackState = (Get-Content -LiteralPath $ack -TotalCount 1).Trim().ToLowerInvariant()
        if ($ackState -eq $State) {
            Write-Output "GOAL_NOTIFICATION_ACK|$GoalId|$State"
            exit 0
        }
        Write-Warning "Goal notification acknowledgment contained unexpected state '$ackState'."
        exit 0
    }
    Start-Sleep -Milliseconds 250
}

Write-Warning "Goal notification acknowledgment was not observed within $AckWaitSeconds seconds (non-fatal)."
exit 0
