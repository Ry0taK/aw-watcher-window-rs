$username = if ($args[0]) { $args[0] } else { $env:USERNAME }
$executable = if ($args[1]) { $args[1] } else { (Get-Command "aw-watcher-window-rs.exe" -ErrorAction SilentlyContinue).Path }
if (-not $executable) {
    Write-Error "aw-watcher-window-rs.exe not found in PATH"
    Pause
    Exit 1
}

if (-NOT ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")) {
    Write-Warning "This script requires administrative privileges. Attempting to elevate..."

    Start-Process powershell.exe "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" `"$username`" `"$executable`"" -Verb RunAs
    Exit
}

$recordTitles = Read-Host "Do you want to record window titles? This may include sensitive information (y/N)"
$excludeTitle = if ($recordTitles.ToLower() -ne 'y') { "--exclude-title" } else { "" }

$hostname = Read-Host "Enter the hostname where aw-server is running (default: localhost)"
if (-not $hostname) {
    $hostname = "localhost"
}

$port = Read-Host "Enter the port where aw-server is running (default: 5600)"
if (-not $port) {
    $port = 5600
}

$args = "--host $hostname --port $port $excludeTitle"

$Action = New-ScheduledTaskAction -Execute $executable -Argument $args
$Trigger = New-ScheduledTaskTrigger -AtLogon -User $username
$Principal = New-ScheduledTaskPrincipal -UserId $username -RunLevel Limited
$Settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit 0

Register-ScheduledTask -TaskName "aw-watcher-window-rs-$username" `
                      -Action $Action `
                      -Trigger $Trigger `
                      -Principal $Principal `
                      -Settings $Settings `
                      -Description "Watches active windows for ActivityWatch"

Write-Host "Task scheduled successfully for user: $username"
Pause

