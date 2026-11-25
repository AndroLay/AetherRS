# uninstall-win12-rust.ps1 - gracefully stop AetherRS and restore folder associations,
# autostart entries, Winlogon\Shell (if pointing to AetherRS), and native Windows Explorer.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File ".\uninstall-win12-rust.ps1" -DryRun
#   powershell -NoProfile -ExecutionPolicy Bypass -File ".\uninstall-win12-rust.ps1"
param(
    [string]$InstallDir = 'C:\Users\andro\Pictures\win12-shell-rust\install',
    [switch]$DryRun,
    [switch]$KeepSettings = $true
)

$ErrorActionPreference = 'Stop'
$proj = Split-Path -Parent $MyInvocation.MyCommand.Path
$installPrefix = [System.IO.Path]::GetFullPath($InstallDir).TrimEnd('\') + '\'
$shellPath = Join-Path $InstallDir 'win12shell.exe'
$assocScript = Join-Path $proj 'win12-files-assoc.ps1'

function Log-Step([string]$msg) {
    if ($DryRun) { Write-Output "[DRY-RUN] $msg" } else { Write-Output $msg }
}

function Is-Admin {
    ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
        [Security.Principal.WindowsBuiltInRole]::Administrator
    )
}

Add-Type -Name NativeUninst -Namespace W12 -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern System.IntPtr FindWindow(string lpClassName, string lpWindowName);
[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern bool ShowWindowAsync(System.IntPtr hWnd, int nCmdShow);
'@

# 1. Stop running win12shell.exe from the install folder gracefully via --exit.
$runningShell = @(Get-CimInstance Win32_Process -Filter "Name='win12shell.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($installPrefix, [System.StringComparison]::OrdinalIgnoreCase) })
if ($runningShell.Count -gt 0) {
    Log-Step "Stopping win12shell.exe (pid $($runningShell[0].ProcessId)) via --exit ..."
    if (-not $DryRun -and (Test-Path -LiteralPath $shellPath)) {
        $p = Start-Process -FilePath $shellPath -ArgumentList '--exit' -PassThru
        [void]$p.WaitForExit(15000)
    }
} else {
    Log-Step 'No active win12shell.exe found in install directory.'
}

# 2. Restore HKCU folder associations.
if (Test-Path -LiteralPath $assocScript) {
    Log-Step 'Disabling folder associations (win12-files-assoc.ps1 -Action Disable) ...'
    if (-not $DryRun) {
        & $assocScript -Action Disable
    }
}

# 3. Remove HKCU Run autostart entries owned by AetherRS.
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
foreach ($name in @('Win12Shell', 'Win12Boot')) {
    $val = (Get-ItemProperty -LiteralPath $runKey -Name $name -ErrorAction SilentlyContinue).$name
    if ($val) {
        Log-Step "Removing HKCU Run\$name ($val)"
        if (-not $DryRun) {
            Remove-ItemProperty -LiteralPath $runKey -Name $name -ErrorAction SilentlyContinue
        }
    }
}

# 4. Remove scheduled tasks owned by AetherRS (if present).
foreach ($taskName in @('Win12Shell', 'Andro-Penjaga-Sumber-Daya')) {
    $t = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    if ($t) {
        Log-Step "Removing scheduled task: $taskName"
        if (-not $DryRun) {
            Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
        }
    }
}

# 5. Check HKCU and HKLM Winlogon\Shell; restore to explorer.exe if pointing to AetherRS.
$wlPaths = @(
    @{ Path = 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Winlogon'; NeedsAdmin = $false; RemoveIfOurs = $true },
    @{ Path = 'HKLM:\Software\Microsoft\Windows NT\CurrentVersion\Winlogon'; NeedsAdmin = $true;  RemoveIfOurs = $false }
)
foreach ($wl in $wlPaths) {
    $cur = (Get-ItemProperty -LiteralPath $wl.Path -Name Shell -ErrorAction SilentlyContinue).Shell
    if ($cur -and ($cur -match 'win12(boot|shell)\.exe')) {
        if ($wl.NeedsAdmin -and -not (Is-Admin)) {
            Log-Step "WARNING: $($wl.Path)\Shell = '$cur' requires an elevated PowerShell session to restore to explorer.exe."
            continue
        }
        if ($wl.RemoveIfOurs) {
            Log-Step "Removing override $($wl.Path)\Shell ($cur)"
            if (-not $DryRun) {
                Remove-ItemProperty -LiteralPath $wl.Path -Name Shell -ErrorAction SilentlyContinue
            }
        } else {
            Log-Step "Restoring $($wl.Path)\Shell from '$cur' to 'explorer.exe'"
            if (-not $DryRun) {
                Set-ItemProperty -LiteralPath $wl.Path -Name Shell -Value 'explorer.exe'
            }
        }
    }
}

# 6. Ensure native Windows taskbar / Explorer is running.
$tray = [W12.NativeUninst]::FindWindow('Shell_TrayWnd', $null)
if ($tray -ne [System.IntPtr]::Zero) {
    Log-Step 'Restoring native Windows taskbar (Shell_TrayWnd).'
    if (-not $DryRun) {
        [void][W12.NativeUninst]::ShowWindowAsync($tray, 4) # SW_SHOWNOACTIVATE
    }
} else {
    Log-Step 'Shell_TrayWnd not found; starting explorer.exe ...'
    if (-not $DryRun) {
        Start-Process -FilePath 'explorer.exe'
    }
}

if ($KeepSettings) {
    Log-Step "User settings in $InstallDir (settings.ini, apps.txt, widgets.ini) were preserved."
}
Log-Step 'Done. Windows has been restored to standard Explorer.'
