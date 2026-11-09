# AlterDNS — Copyright (C) 2026 Furkan Okan
# SPDX-License-Identifier: GPL-3.0-or-later
#
# UI spike davranış denetimi (T-SPK-001; UI_SPEC §4). Windows PowerShell 5.1 ile çalışır
# (UIAutomationClient). Ölçüm kipi açıktır (canlı masaüstünde odak değişimi pencereyi kapatmasın);
# odak kaybında kapanma elle denetlenir.
# Kullanım: powershell.exe -NoProfile -File behavior-check.ps1 -Exe <yol> [-ExtraArg <a>] -Label <ad> -OutDir <dizin>
param(
    [Parameter(Mandatory)] [string] $Exe,
    [string] $ExtraArg = "",
    [Parameter(Mandatory)] [string] $Label,
    [Parameter(Mandatory)] [string] $OutDir
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class W {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string n);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern uint RegisterWindowMessage(string s);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr h, int i);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    public static IntPtr FlyoutOf(uint pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            var sb = new StringBuilder(256); GetClassName(h, sb, 256);
            if (p == pid && IsWindowVisible(h) && sb.ToString() != "AlterDNSSpikeTray") { found = h; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
"@

New-Item -ItemType Directory -Force $OutDir | Out-Null
$psi = New-Object System.Diagnostics.ProcessStartInfo $Exe
if ($ExtraArg) { $psi.Arguments = $ExtraArg }
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.EnvironmentVariables["ALTERDNS_SPIKE_MEASURE"] = "1"
$proc = [System.Diagnostics.Process]::Start($psi)
$lines = New-Object System.Collections.Concurrent.ConcurrentQueue[string]
$null = Register-ObjectEvent -InputObject $proc -EventName OutputDataReceived -Action { if ($EventArgs.Data) { $Event.MessageData.Enqueue($EventArgs.Data) } } -MessageData $lines
$proc.BeginOutputReadLine()
function Wait-Line([string]$prefix, [int]$ms = 15000) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt $ms) {
        $l = $null
        if ($lines.TryDequeue([ref]$l) -and $l.StartsWith($prefix)) { return $l }
        Start-Sleep -Milliseconds 20
    }
    return $null
}
$result = [ordered]@{ label = $Label }
try {
    if (-not (Wait-Line "TRAY_READY")) { throw "TRAY_READY gelmedi" }
    $tray = [W]::FindWindow("AlterDNSSpikeTray", $null)
    $cmd = [W]::RegisterWindowMessage("AlterDNS.Spike.Toggle")
    [void][W]::PostMessage($tray, $cmd, [IntPtr]1, [IntPtr]0)
    $ready = Wait-Line "READY "
    $result.opened = [bool]$ready
    if (-not $ready) { throw "READY gelmedi" }
    $h = [IntPtr][int64]($ready.Split(' ')[2])
    Start-Sleep -Milliseconds 400

    $style = [int64][W]::GetWindowLongPtr($h, -16); $ex = [int64][W]::GetWindowLongPtr($h, -20)
    $result.no_caption = (($style -band 0x00C00000) -eq 0)
    $result.no_sysmenu = (($style -band 0x00080000) -eq 0)
    $result.toolwindow = (($ex -band 0x80) -ne 0)
    $result.topmost = (($ex -band 0x8) -ne 0)

    $r = New-Object W+RECT; [void][W]::GetWindowRect($h, [ref]$r)
    $result.rect = "{0},{1} {2}x{3}" -f $r.Left, $r.Top, ($r.Right - $r.Left), ($r.Bottom - $r.Top)
    $pt = [IntPtr](([int64](($r.Top + 3) -band 0xFFFF) -shl 16) -bor ([int64](($r.Left + 20) -band 0xFFFF)))
    $result.nchittest_top_edge = [int64][W]::SendMessage($h, 0x0084, [IntPtr]::Zero, $pt)   # 1 = HTCLIENT

    [void][W]::PostMessage($h, 0x0112, [IntPtr]0xF010, [IntPtr]::Zero)   # WM_SYSCOMMAND SC_MOVE
    [void][W]::PostMessage($h, 0x0112, [IntPtr]0xF100, [IntPtr]0x20)     # SC_KEYMENU (Alt+Space)
    Start-Sleep -Milliseconds 400
    $r2 = New-Object W+RECT; [void][W]::GetWindowRect($h, [ref]$r2)
    $result.not_moved = ($r.Left -eq $r2.Left -and $r.Top -eq $r2.Top)

    $bmp = New-Object System.Drawing.Bitmap ($r.Right - $r.Left), ($r.Bottom - $r.Top)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
    $png = Join-Path $OutDir "$Label.png"; $bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
    $result.screenshot = $png

    $el = [System.Windows.Automation.AutomationElement]::FromHandle($h)
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $uia = New-Object System.Collections.Generic.List[string]
    function Walk($e, $d) {
        if ($d -gt 6 -or $uia.Count -gt 60) { return }
        $uia.Add(("{0}{1} '{2}'" -f ('  ' * $d), $e.Current.ControlType.ProgrammaticName, $e.Current.Name))
        $c = $walker.GetFirstChild($e)
        while ($c -ne $null) { Walk $c ($d + 1); $c = $walker.GetNextSibling($c) }
    }
    Walk $el 0
    $result.uia_nodes = $uia.Count
    $uia | Set-Content -Encoding UTF8 (Join-Path $OutDir "$Label-uia.txt")

    [void][W]::PostMessage($h, 0x0100, [IntPtr]0x1B, [IntPtr]::Zero)   # WM_KEYDOWN VK_ESCAPE
    $result.esc_closes = [bool](Wait-Line "HIDDEN" 3000)
}
catch { $result.error = $_.Exception.Message }
finally { if (-not $proc.HasExited) { $proc.Kill() }; $proc.WaitForExit(3000) | Out-Null }
$result | ConvertTo-Json -Compress
