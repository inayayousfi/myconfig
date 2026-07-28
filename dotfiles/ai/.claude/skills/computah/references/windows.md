# computah — Windows

## Contents

- Prerequisites
- Invocation (native agent, WSL/Git Bash, script-file fallback)
- The preamble — required, ordering matters
- Capture the screen
- Crop a capture
- Move, click, drag, scroll
- Type text and press keys
- Launch and focus by keyboard
- Diagnosing a silent no-op
- Gotchas

## Prerequisites

- `ExecutionPolicy` is `Bypass` at LocalMachine, so scripts need no signing.
- Works identically on Windows PowerShell 5.1 and pwsh 7.x. Both are STA by
  default, so `-STA` is harmless but unnecessary.
- The agent's session must be attached to the interactive desktop. If a capture
  comes back entirely black, it is not, and nothing in this file will work.

## Invocation

The PowerShell payload below is host-independent. Only the wrapper changes.

**Native Windows agent** — issue the payload directly to the PowerShell tool. No
wrapper, no escaping layer.

**From WSL or Git Bash** — wrap in **single** quotes so the shell leaves the
payload alone:

```bash
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command '<payload>'
```

**Fallback** — when quoting gets awkward, write the payload to a `.ps1` under a
Windows-visible path and run `-File`. Note that Windows cannot traverse WSL
symlinks over UNC, so any path handed to `powershell.exe` must be a resolved
real path.

**Never inline-quote C#.** Always use an `@"…"@` here-string. `\"` is not a
PowerShell escape (the backtick is), and backslash-escaped C# fails with a
misleading `A positional parameter cannot be found` error.

## The preamble — required, ordering matters

Every invocation is a fresh process, so every invocation must repeat this.

```powershell
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, IntPtr extra);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
}
"@
[void][W]::SetProcessDPIAware()
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
```

`SetProcessDPIAware()` is **per-process and one-shot**, and .NET caches screen
metrics on first access. It must run *after* the P/Invoke `Add-Type` (which does
not touch WinForms) but *before* `Add-Type -AssemblyName System.Windows.Forms`
and before any `Screen` or `SystemInformation` read.

Get this order wrong and coordinates silently mix logical space with physical
space. On a 150%-scaled display every click then lands a third of the screen
away, which looks like an aiming problem and is not.

## Capture the screen

Capture uses the screen-copy method of the drawing API
(`Graphics.CopyFromScreen`). It does not use a key press.

Mouse and keyboard govern how you **act**. They do not govern how you **look**.
Windows actively defends against synthetic capture keys, so a key press here
buys nothing and breaks often.

```powershell
$b = [System.Windows.Forms.SystemInformation]::VirtualScreen
$bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($b.X, $b.Y, 0, 0, $bmp.Size)
$g.Dispose()
$out = Join-Path $env:TEMP "computah\shot.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
"$($bmp.Width)x$($bmp.Height) -> $out"
$bmp.Dispose()
```

This route never touches the clipboard. Nothing the user copied gets destroyed.

**Validate the pixels, not the dimensions.** A capture can report the correct
size and still contain no image at all. Probe the mean brightness before
trusting it.

```powershell
$sum = 0; $n = 0
for ($y = 0; $y -lt $bmp.Height; $y += 97) {
  for ($x = 0; $x -lt $bmp.Width; $x += 97) {
    $p = $bmp.GetPixel($x, $y); $sum += ($p.R + $p.G + $p.B) / 3; $n++
  }
}
"mean brightness $([int]($sum / $n))"
```

A mean of 0 means the capture failed. A dark desktop still reads about 28.

**Why the PrintScreen key is not used.** Three behaviours, all verified on this
machine.

- Bare PrintScreen opens the Snipping Tool on Windows 11. It puts nothing on
  the clipboard. It leaves a `SnippingTool` process running.
- Ctrl+PrintScreen bypasses that binding. It returns a clipboard image of the
  correct size with no pixel data. Every pixel is black.
- Alt+PrintScreen returns real pixels. It captures only the active window.

Use Alt+PrintScreen as the escape hatch when `CopyFromScreen` returns black.
Some protected windows refuse one method but allow the other.

## Crop a capture

For zooming in on a small target, and for cheap verification of a known region.
Operates on the file already captured — it does not take a new one.

```powershell
$in = Join-Path $env:TEMP "computah-shot.png"
$out = Join-Path $env:TEMP "computah-crop.png"
$x = 800; $y = 400; $w = 400; $h = 300      # region in capture pixels

$src  = [System.Drawing.Image]::FromFile($in)
$crop = New-Object System.Drawing.Bitmap $w, $h
$g    = [System.Drawing.Graphics]::FromImage($crop)
$g.DrawImage($src,
  (New-Object System.Drawing.Rectangle 0, 0, $w, $h),
  (New-Object System.Drawing.Rectangle $x, $y, $w, $h),
  [System.Drawing.GraphicsUnit]::Pixel)
$g.Dispose(); $src.Dispose()
$crop.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$crop.Dispose()
"cropped ${w}x${h} at $x,$y -> $out"
```

Coordinates found in the crop are crop-relative. Add `$x`/`$y` back to get
capture coordinates, which are already screen coordinates.

## Move, click, drag, scroll

```powershell
# move + left click
[void][W]::SetCursorPos(413, 494)
Start-Sleep -Milliseconds 250
[W]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)   # LEFTDOWN
Start-Sleep -Milliseconds 60
[W]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)   # LEFTUP
```

Event flags: `LEFTDOWN 0x0002`, `LEFTUP 0x0004`, `RIGHTDOWN 0x0008`,
`RIGHTUP 0x0010`, `MIDDLEDOWN 0x0020`, `MIDDLEUP 0x0040`, `WHEEL 0x0800`.

Double click: two down/up pairs about 80ms apart. Drag: `SetCursorPos` to start,
`LEFTDOWN`, `SetCursorPos` to end (a couple of intermediate moves help apps that
track motion), then `LEFTUP`.

Scroll — `$d` is 120 per notch, positive scrolls up:

```powershell
[W]::mouse_event(0x0800, 0, 0, -360, [IntPtr]::Zero)   # three notches down
```

## Type text and press keys

```powershell
[System.Windows.Forms.SendKeys]::SendWait("hello there")
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
```

Modifiers: `+` shift, `^` ctrl, `%` alt — e.g. `^a`, `%{F4}`.
Named keys: `{ENTER} {TAB} {ESC} {BACKSPACE} {DELETE} {HOME} {END} {UP} {DOWN}
{LEFT} {RIGHT} {F1}`.

The characters `+ ^ % ~ ( ) [ ] { }` are special to `SendKeys`. To type one
literally, wrap it in braces: `{+}`, `{%}`, `{(}`.

`SendKeys` cannot press the Windows key. Use `keybd_event` with `VK_LWIN`
(0x5B), as below.

## Launch and focus by keyboard

```powershell
[W]::keybd_event(0x5B, 0, 0, [IntPtr]::Zero)   # Win down
[W]::keybd_event(0x5B, 0, 2, [IntPtr]::Zero)   # Win up
Start-Sleep -Milliseconds 900
[System.Windows.Forms.SendKeys]::SendWait("chrome")
Start-Sleep -Milliseconds 1400
```

**Now capture and confirm the highlighted result is the intended application
before sending `{ENTER}`.** The top hit varies with search history. This is the
one step where skipping verification reliably ruins the whole chain.

Alt+Tab, held long enough for the switcher to appear:

```powershell
[W]::keybd_event(0x12, 0, 0, [IntPtr]::Zero)   # Alt down
[W]::keybd_event(0x09, 0, 0, [IntPtr]::Zero)   # Tab down
[W]::keybd_event(0x09, 0, 2, [IntPtr]::Zero)   # Tab up
Start-Sleep -Milliseconds 600
[W]::keybd_event(0x12, 0, 2, [IntPtr]::Zero)   # Alt up
```

## Diagnosing a silent no-op

When verification shows nothing changed, check elevation before re-aiming. A
non-elevated process cannot send input to an elevated window — UIPI drops it
with no error at all.

```powershell
$h = [W]::GetForegroundWindow()
$fpid = 0; [void][W]::GetWindowThreadProcessId($h, [ref]$fpid)
$me = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
      ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
"agent elevated: $me ; foreground pid: $fpid"
```

If the agent is not elevated and the target window is, say so and stop. Retrying
will never work.

## Gotchas

**Bare PrintScreen is hijacked by the Snipping Tool on Windows 11.** It opens an
overlay and puts nothing on the clipboard. Always hold Ctrl (full desktop) or
Alt (active window). The registry flag is
`HKCU:\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled` — unset means
the modern default, which is enabled. Do not change it; use the modifier.

**The cursor is not in the capture.** `CopyFromScreen` and PrintScreen do not
draw the mouse pointer. Do not try to locate the pointer visually — track its
position in state, since it is always set explicitly.

**UIPI silently drops input to elevated windows.** No error, no exception,
nothing happens. See the diagnostic above.

**Always-on-top windows are captured over the target.** The on-screen keyboard
(`osk.exe`) is a common offender. Neither full-screen nor Alt+PrintScreen avoids
it. Close or move the overlay, or work around the occluded region.

**A capture that reports the right size can still be entirely black.** Validate
brightness, not dimensions. This is how the clipboard capture route was found to
be broken after it appeared to succeed.

**`Get-Clipboard -Format Image` does not exist in pwsh 7.** It was removed after
5.1. Use the clipboard image accessor of the forms API
(`[System.Windows.Forms.Clipboard]::GetImage()`). It works on both versions.
Only relevant for the Alt+PrintScreen escape hatch.

**`ShowWindow(h, 9)` (`SW_RESTORE`) un-maximizes a maximized window.** Use
`SW_SHOW` (5) if a window must be shown.

**Budget ~900ms per PowerShell invocation**, and about 2s per action once settle
delays are included. Do not poll in a tight loop.

**Reference display on this machine:** single monitor, 150% scaling,
`1707x1067` logical, `2560x1600` physical, origin `0,0`. DPI-aware captures come
out `2560x1600`, which an image-reading tool clamped to `2000x1250` reports with
a factor of 1.28. Treat these as the current values, not constants — read the
factor from the tool's output every time.
