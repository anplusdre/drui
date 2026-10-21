param([int64]$Handle, [string]$Out)
# full-resolution capture of one window (PrintWindow with PW_RENDERFULLCONTENT), DPI-aware; prints size and DPI
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class W {
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  public struct RECT { public int L, T, R, B; }
  public struct POINT { public int X, Y; }
}
"@
[W]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null
$h = [IntPtr]$Handle
$r = New-Object W+RECT; [W]::GetWindowRect($h, [ref]$r) | Out-Null
$c = New-Object W+RECT; [W]::GetClientRect($h, [ref]$c) | Out-Null
$p = New-Object W+POINT; [W]::ClientToScreen($h, [ref]$p) | Out-Null
$w = $r.R - $r.L; $hh = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $hh
$g = [System.Drawing.Graphics]::FromImage($bmp)
$dc = $g.GetHdc(); [W]::PrintWindow($h, $dc, 2) | Out-Null; $g.ReleaseHdc($dc); $g.Dispose()
# crop to the client area
$cx = $p.X - $r.L; $cy = $p.Y - $r.T
$crop = $bmp.Clone((New-Object System.Drawing.Rectangle $cx, $cy, $c.R, $c.B), $bmp.PixelFormat)
$crop.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
"window ${w}x${hh} client $($c.R)x$($c.B) dpi $([W]::GetDpiForWindow($h))"
