Add-Type @"
using System;
using System.Runtime.InteropServices;
[ComImport, Guid("a5cd92ff-29be-454c-8d04-d82879fb3f1b"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IVirtualDesktopManager {
  [PreserveSig] int IsWindowOnCurrentVirtualDesktop(IntPtr h, out bool on);
  [PreserveSig] int GetWindowDesktopId(IntPtr h, out Guid id);
  [PreserveSig] int MoveWindowToDesktop(IntPtr h, ref Guid id);
}
[ComImport, Guid("aa509086-5ca9-4c25-8f95-589d3c07b48a")] public class VDMClass {}
public static class V {
  static IVirtualDesktopManager m = (IVirtualDesktopManager)new VDMClass();
  public static bool OnCurrent(IntPtr h) { bool b; m.IsWindowOnCurrentVirtualDesktop(h, out b); return b; }
}
"@
Get-Process Figma, drui | Where-Object { $_.MainWindowHandle -ne 0 } | ForEach-Object {
  "{0} pid={1} hwnd={2} onCurrentDesktop={3}" -f $_.ProcessName, $_.Id, $_.MainWindowHandle, [V]::OnCurrent($_.MainWindowHandle)
}
