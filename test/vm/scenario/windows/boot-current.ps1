# Prints the firmware boot entry that started this boot, as "Boot<number> <label>": the
# BootCurrent firmware variable and the description of the Boot#### entry it names. Reading
# firmware variables needs SeSystemEnvironmentPrivilege, which administrators hold but must
# turn on in their token first.

$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class InayosFirmware {
    const string GlobalVariables = "{8BE4DF61-93CA-11D2-AA0D-00E098032B8C}";

    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    struct Privileges {
        public uint Count;
        public long Luid;
        public uint Attributes;
    }

    [DllImport("kernel32.dll")]
    static extern IntPtr GetCurrentProcess();

    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);

    [DllImport("advapi32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool LookupPrivilegeValueW(string system, string name, out long luid);

    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool AdjustTokenPrivileges(IntPtr token, bool disableAll, ref Privileges state,
        uint length, IntPtr previous, IntPtr returnLength);

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern uint GetFirmwareEnvironmentVariableW(string name, string guid, byte[] buffer,
        uint size);

    public static void EnablePrivilege() {
        IntPtr token;
        // TOKEN_ADJUST_PRIVILEGES and TOKEN_QUERY.
        if (!OpenProcessToken(GetCurrentProcess(), 0x28, out token)) throw new Win32Exception();
        Privileges state = new Privileges();
        state.Count = 1;
        // SE_PRIVILEGE_ENABLED.
        state.Attributes = 2;
        if (!LookupPrivilegeValueW(null, "SeSystemEnvironmentPrivilege", out state.Luid)) {
            throw new Win32Exception();
        }
        if (!AdjustTokenPrivileges(token, false, ref state, 0, IntPtr.Zero, IntPtr.Zero)) {
            throw new Win32Exception();
        }
        // AdjustTokenPrivileges succeeds without the privilege and says so in the last error.
        int error = Marshal.GetLastWin32Error();
        if (error != 0) throw new Win32Exception(error);
    }

    public static byte[] Read(string name) {
        byte[] buffer = new byte[4096];
        uint size = GetFirmwareEnvironmentVariableW(name, GlobalVariables, buffer, (uint)buffer.Length);
        if (size == 0) throw new Win32Exception();
        Array.Resize(ref buffer, (int)size);
        return buffer;
    }
}
'@

[InayosFirmware]::EnablePrivilege()
$number = [BitConverter]::ToUInt16([InayosFirmware]::Read('BootCurrent'), 0)
$name = 'Boot{0:X4}' -f $number
$option = [InayosFirmware]::Read($name)
# An EFI load option starts with 4 bytes of attributes and 2 bytes of device path length,
# then the description as a UTF-16 string that ends with a zero character.
$end = 6
while ($end + 1 -lt $option.Length -and ($option[$end] -ne 0 -or $option[$end + 1] -ne 0)) {
    $end += 2
}
$label = [Text.Encoding]::Unicode.GetString($option, 6, $end - 6)
Write-Output "$name $label"
