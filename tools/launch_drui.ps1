# starts THIS project's drui at normal rights (not as administrator), however elevated the calling shell is: an elevated drui blocks the
# Snipping Tool / Print Screen shortcut while it has focus, because Windows will not hand that key to a lower-rights program.
# Usage: launch_drui.ps1 [file.json] [flags]   (the window is shown normally; relaunch_drui.ps1 adds --background for automation)
$exe = 'D:\dev\drui\target\debug\drui.exe'
$lnk = Join-Path $env:TEMP 'drui-launch.lnk'
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$s.TargetPath = $exe
$s.Arguments = (@($args) | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' '
$s.WorkingDirectory = Split-Path $exe
$s.Save()
# explorer starts it, so it gets explorer's rights
Start-Process explorer.exe -ArgumentList $lnk
Start-Sleep 3
Remove-Item $lnk -Force -ErrorAction SilentlyContinue
