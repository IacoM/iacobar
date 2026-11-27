# Emergency restore of the native Windows taskbar.
# Use it only if Iacobar was killed (e.g. from Task Manager) and the
# Windows taskbar is still hidden. Usage:  powershell -File restore-taskbar.ps1

Get-Process iacobar -ErrorAction SilentlyContinue | Stop-Process -Force

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Tb {
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT { public int l, t, r, b; }
  [StructLayout(LayoutKind.Sequential)]
  public struct APPBARDATA {
    public int cbSize; public IntPtr hWnd; public uint uCallbackMessage;
    public uint uEdge; public RECT rc; public IntPtr lParam;
  }
  [DllImport("shell32.dll")] public static extern UIntPtr SHAppBarMessage(uint msg, ref APPBARDATA d);
  [DllImport("user32.dll")] public static extern IntPtr FindWindow(string cls, string name);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  public static void Restore() {
    var d = new APPBARDATA();
    d.cbSize = Marshal.SizeOf(d);
    d.hWnd = FindWindow("Shell_TrayWnd", null);
    d.lParam = IntPtr.Zero;              // ABS_ALWAYSONTOP off, AUTOHIDE off
    SHAppBarMessage(0x0A, ref d);        // ABM_SETSTATE
    ShowWindow(d.hWnd, 5);               // SW_SHOW
  }
}
"@
[Tb]::Restore()
Write-Host "Barra di Windows ripristinata. Se non compare, riavvia Esplora risorse da Gestione attivita'."
