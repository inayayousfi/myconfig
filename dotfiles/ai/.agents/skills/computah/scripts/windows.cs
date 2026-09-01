using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows.Forms;

internal static class Computah
{
    private static bool perMonitorV2Enabled;

    private const uint LeftDown = 0x0002;
    private const uint LeftUp = 0x0004;
    private const uint RightDown = 0x0008;
    private const uint RightUp = 0x0010;
    private const uint MiddleDown = 0x0020;
    private const uint MiddleUp = 0x0040;
    private const uint Wheel = 0x0800;
    private const uint KeyUp = 0x0002;

    [DllImport("user32.dll")] private static extern bool SetProcessDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll")] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll")] private static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] private static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] private static extern void mouse_event(uint flags, uint x, uint y, uint data, IntPtr extra);
    [DllImport("user32.dll")] private static extern void keybd_event(byte key, byte scan, uint flags, IntPtr extra);
    [DllImport("user32.dll")] private static extern short VkKeyScan(char character);

    [STAThread]
    public static int Main(string[] args)
    {
        EnableDpiAwareness();
        try
        {
            if (args.Length == 0) throw new ArgumentException("missing command");
            string command = args[0].ToLowerInvariant();
            switch (command)
            {
                case "capabilities":
                    Capabilities();
                    break;
                case "capture": Capture(args.Length > 1 ? args[1] : null); break;
                case "crop": Crop(args); break;
                case "move": Require(args, 3); SetCursor(int.Parse(args[1]), int.Parse(args[2])); break;
                case "click": Click(args); break;
                case "drag": Drag(args); break;
                case "scroll": Require(args, 2); Scroll(int.Parse(args[1])); break;
                case "type": Require(args, 2); SendKeys.SendWait(EscapeSendKeys(args[1])); break;
                case "key": Require(args, 2); SendChord(args[1]); break;
                default: throw new ArgumentException("unknown command: " + args[0]);
            }
            return 0;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine("computah: " + error.Message);
            return 2;
        }
    }

    private static void EnableDpiAwareness()
    {
        try
        {
            if (SetProcessDpiAwarenessContext(new IntPtr(-4)))
            {
                perMonitorV2Enabled = true;
                return;
            }
            if (SetThreadDpiAwarenessContext(new IntPtr(-4)) != IntPtr.Zero)
            {
                perMonitorV2Enabled = true;
                return;
            }
        }
        catch (EntryPointNotFoundException)
        {
            // Windows versions before 10 do not expose Per-Monitor V2.
        }
        SetProcessDPIAware();
    }

    private static void Capabilities()
    {
        Rectangle bounds = SystemInformation.VirtualScreen;
        Console.WriteLine(
            "{\"platform\":\"windows\",\"coordinateSpace\":\"capture-pixels\"," +
            "\"virtualOriginX\":" + bounds.X + ",\"virtualOriginY\":" + bounds.Y + "," +
            "\"width\":" + bounds.Width + ",\"height\":" + bounds.Height + "," +
            "\"dpiAwareness\":\"" + (perMonitorV2Enabled ? "per-monitor-v2" : "system") +
            "\",\"capture\":\"win32\"," +
            "\"crop\":\"system-drawing\",\"keyboard\":\"win32\"," +
            "\"pointer\":\"win32\",\"scroll\":\"win32\"}"
        );
    }

    private static void Capture(string output)
    {
        Rectangle bounds = SystemInformation.VirtualScreen;
        string path = FullOutputPath(output ?? Path.Combine(Path.GetTempPath(), "computah", "shot.png"));
        using (Bitmap bitmap = new Bitmap(bounds.Width, bounds.Height, PixelFormat.Format32bppArgb))
        using (Graphics graphics = Graphics.FromImage(bitmap))
        {
            graphics.CopyFromScreen(bounds.X, bounds.Y, 0, 0, bitmap.Size);
            bitmap.Save(path, ImageFormat.Png);
        }
        Console.WriteLine(path);
    }

    private static void Crop(string[] args)
    {
        Require(args, 7);
        string input = Path.GetFullPath(args[1]);
        string output = FullOutputPath(args[2]);
        Rectangle area = new Rectangle(int.Parse(args[3]), int.Parse(args[4]), int.Parse(args[5]), int.Parse(args[6]));
        using (Image source = Image.FromFile(input))
        using (Bitmap crop = new Bitmap(area.Width, area.Height, PixelFormat.Format32bppArgb))
        using (Graphics graphics = Graphics.FromImage(crop))
        {
            graphics.DrawImage(source, new Rectangle(0, 0, area.Width, area.Height), area, GraphicsUnit.Pixel);
            crop.Save(output, ImageFormat.Png);
        }
        Console.WriteLine(output);
    }

    private static void Click(string[] args)
    {
        Require(args, 3);
        SetCursor(int.Parse(args[1]), int.Parse(args[2]));
        Thread.Sleep(80);
        MouseButton(args.Length > 3 ? args[3] : "left", true);
        Thread.Sleep(50);
        MouseButton(args.Length > 3 ? args[3] : "left", false);
    }

    private static void Drag(string[] args)
    {
        Require(args, 5);
        int x1 = int.Parse(args[1]), y1 = int.Parse(args[2]);
        int x2 = int.Parse(args[3]), y2 = int.Parse(args[4]);
        string button = args.Length > 5 ? args[5] : "left";
        SetCursor(x1, y1);
        MouseButton(button, true);
        for (int i = 1; i <= 12; i++)
        {
            SetCursor(x1 + (x2 - x1) * i / 12, y1 + (y2 - y1) * i / 12);
            Thread.Sleep(15);
        }
        MouseButton(button, false);
    }

    private static void SetCursor(int x, int y)
    {
        Rectangle bounds = SystemInformation.VirtualScreen;
        if (x < 0 || x >= bounds.Width || y < 0 || y >= bounds.Height)
            throw new ArgumentOutOfRangeException("coordinates", "coordinates fall outside the captured desktop");
        if (!SetCursorPos(bounds.X + x, bounds.Y + y))
            throw new InvalidOperationException("SetCursorPos failed");
    }

    private static void MouseButton(string name, bool down)
    {
        uint flag;
        switch (name.ToLowerInvariant())
        {
            case "left": flag = down ? LeftDown : LeftUp; break;
            case "right": flag = down ? RightDown : RightUp; break;
            case "middle": flag = down ? MiddleDown : MiddleUp; break;
            default: throw new ArgumentException("unknown mouse button: " + name);
        }
        mouse_event(flag, 0, 0, 0, IntPtr.Zero);
    }

    private static void Scroll(int notches)
    {
        mouse_event(Wheel, 0, 0, unchecked((uint)(notches * 120)), IntPtr.Zero);
    }

    private static string EscapeSendKeys(string text)
    {
        const string special = "+^%~()[]{}";
        string result = "";
        foreach (char character in text)
            result += special.IndexOf(character) >= 0 ? "{" + character + "}" : character.ToString();
        return result;
    }

    private static void SendChord(string chord)
    {
        string[] names = chord.Split('+');
        var modifiers = new List<byte>();
        for (int i = 0; i < names.Length - 1; i++) modifiers.Add(VirtualKey(names[i]));
        byte key = VirtualKey(names[names.Length - 1]);
        foreach (byte modifier in modifiers) keybd_event(modifier, 0, 0, IntPtr.Zero);
        keybd_event(key, 0, 0, IntPtr.Zero);
        keybd_event(key, 0, KeyUp, IntPtr.Zero);
        for (int i = modifiers.Count - 1; i >= 0; i--) keybd_event(modifiers[i], 0, KeyUp, IntPtr.Zero);
    }

    private static byte VirtualKey(string name)
    {
        switch (name.Trim().ToUpperInvariant())
        {
            case "CTRL": case "CONTROL": return 0x11;
            case "ALT": return 0x12;
            case "SHIFT": return 0x10;
            case "META": case "WIN": case "SUPER": return 0x5B;
            case "ENTER": case "RETURN": return 0x0D;
            case "TAB": return 0x09;
            case "ESC": case "ESCAPE": return 0x1B;
            case "BACKSPACE": return 0x08;
            case "DELETE": return 0x2E;
            case "HOME": return 0x24;
            case "END": return 0x23;
            case "LEFT": return 0x25;
            case "UP": return 0x26;
            case "RIGHT": return 0x27;
            case "DOWN": return 0x28;
            case "SPACE": return 0x20;
        }
        if (name.Length == 1)
        {
            short value = VkKeyScan(name[0]);
            if (value != -1) return (byte)(value & 0xff);
        }
        if (name.Length > 1 && name[0] == 'F')
        {
            int number;
            if (int.TryParse(name.Substring(1), out number) && number >= 1 && number <= 24)
                return (byte)(0x70 + number - 1);
        }
        throw new ArgumentException("unknown key: " + name);
    }

    private static string FullOutputPath(string path)
    {
        string full = Path.GetFullPath(path);
        Directory.CreateDirectory(Path.GetDirectoryName(full));
        return full;
    }

    private static void Require(string[] args, int count)
    {
        if (args.Length < count) throw new ArgumentException("not enough arguments for " + args[0]);
    }
}
