#Requires AutoHotkey v2.0
#SingleInstance Force
InstallKeybdHook()
#MaxThreadsPerHotkey 4

Persistent
CoordMode "Mouse", "Screen"
HOLD_THRESHOLD := 400  ; ms

homeRowActive      := false
disabledModeActive := false
valoModeActive     := false

keyStates := Map()

A_TrayMenu.Delete()
A_TrayMenu.Add("Home Row Mods : OFF", ToggleHomeRow)
A_TrayMenu.Add("Disabled Mode : OFF", ToggleDisabledMode)
A_TrayMenu.Add("Valo Mode : OFF", ToggleValoMode)
A_TrayMenu.Add()
A_TrayMenu.Add("Quitter", (*) => ExitApp())

ToggleHomeRow(*) {
    global homeRowActive, disabledModeActive, valoModeActive
    if disabledModeActive {
        disabledModeActive := false
        A_TrayMenu.Rename("Disabled Mode : ON ✅", "Disabled Mode : OFF")
        SetDisabledModeHotkeys("Off")
    }
    if valoModeActive {
        valoModeActive := false
        A_TrayMenu.Rename("Valo Mode : ON ✅", "Valo Mode : OFF")
        SetValoModeHotkeys("Off")
    }
    homeRowActive := !homeRowActive
    if homeRowActive {
        A_TrayMenu.Rename("Home Row Mods : OFF", "Home Row Mods : ON ✅")
        SetHomeRowHotkeys("On")
    } else {
        A_TrayMenu.Rename("Home Row Mods : ON ✅", "Home Row Mods : OFF")
        SetHomeRowHotkeys("Off")
    }
}

ToggleDisabledMode(*) {
    global homeRowActive, disabledModeActive, valoModeActive
    if homeRowActive {
        homeRowActive := false
        A_TrayMenu.Rename("Home Row Mods : ON ✅", "Home Row Mods : OFF")
        SetHomeRowHotkeys("Off")
    }
    if valoModeActive {
        valoModeActive := false
        A_TrayMenu.Rename("Valo Mode : ON ✅", "Valo Mode : OFF")
        SetValoModeHotkeys("Off")
    }
    disabledModeActive := !disabledModeActive
    if disabledModeActive {
        A_TrayMenu.Rename("Disabled Mode : OFF", "Disabled Mode : ON ✅")
        SetDisabledModeHotkeys("On")
    } else {
        A_TrayMenu.Rename("Disabled Mode : ON ✅", "Disabled Mode : OFF")
        SetDisabledModeHotkeys("Off")
    }
}

ToggleValoMode(*) {
    global homeRowActive, disabledModeActive, valoModeActive
    if homeRowActive {
        homeRowActive := false
        A_TrayMenu.Rename("Home Row Mods : ON ✅", "Home Row Mods : OFF")
        SetHomeRowHotkeys("Off")
    }
    if disabledModeActive {
        disabledModeActive := false
        A_TrayMenu.Rename("Disabled Mode : ON ✅", "Disabled Mode : OFF")
        SetDisabledModeHotkeys("Off")
    }
    valoModeActive := !valoModeActive
    if valoModeActive {
        A_TrayMenu.Rename("Valo Mode : OFF", "Valo Mode : ON ✅")
        SetValoModeHotkeys("On")
    } else {
        A_TrayMenu.Rename("Valo Mode : ON ✅", "Valo Mode : OFF")
        SetValoModeHotkeys("Off")
    }
}

HandleModTap(key, modDown, modUp, waitKey := "") {
    global keyStates, HOLD_THRESHOLD

    wk := (waitKey != "") ? waitKey : key

    if keyStates.Has(key)
        return
    keyStates[key] := true

    startTime := A_TickCount
    isHold := false

    while GetKeyState(wk, "P") {
        if (A_TickCount - startTime >= HOLD_THRESHOLD) {
            isHold := true
            break
        }
        Sleep(10)
    }

    if (isHold) {
        SendEvent(modDown)
        KeyWait(wk)
        SendEvent(modUp)
    } else {
        hotkeyName := "*$" . key
        HotKey(hotkeyName, "Off")
        SendEvent("{" . key . "}")
        HotKey(hotkeyName, "On")
    }

    keyStates.Delete(key)
}

; Tap envoie tapKey, Hold envoie modDown/modUp
HandleModTapCustom(key, tapKey, modDown, modUp, waitKey := "") {
    global keyStates, HOLD_THRESHOLD

    wk := (waitKey != "") ? waitKey : key

    if keyStates.Has(key)
        return
    keyStates[key] := true

    startTime := A_TickCount
    isHold := false

    while GetKeyState(wk, "P") {
        if (A_TickCount - startTime >= HOLD_THRESHOLD) {
            isHold := true
            break
        }
        Sleep(10)
    }

    if (isHold) {
        SendEvent(modDown)
        KeyWait(wk)
        SendEvent(modUp)
    } else {
        SendEvent(tapKey)
    }

    keyStates.Delete(key)
}

SetHomeRowHotkeys(state) {
    for key, mods in Map(
        "a", ["{LWin Down}", "{LWin Up}"],
        "s", ["{LAlt Down}", "{LAlt Up}"],
        "d", ["{LCtrl Down}", "{LCtrl Up}"],
        "f", ["{LShift Down}", "{LShift Up}"],
        "j", ["{RShift Down}", "{RShift Up}"],
        "k", ["{RCtrl Down}", "{RCtrl Up}"],
        "l", ["{RAlt Down}", "{RAlt Up}"],
        ";", ["{RWin Down}", "{RWin Up}"]
    ) {
        modDown := mods[1], modUp := mods[2]
        HotKey(
            "*$" . key,
            ((k, md, mu) => (*) => HandleModTap(k, md, mu))(key, modDown, modUp),
            state
        )
    }
}

SetDisabledModeHotkeys(state) {
    for key, mods in Map(
        "z",     ["{LWin Down}", "{LWin Up}"],
        "a",     ["{LAlt Down}", "{LAlt Up}"],
        "s",     ["{LCtrl Down}", "{LCtrl Up}"],
        "w",     ["{LShift Down}", "{LShift Up}"]
    ) {
        modDown := mods[1], modUp := mods[2]
        HotKey(
            "*$" . key,
            ((k, md, mu) => (*) => HandleModTap(k, md, mu))(key, modDown, modUp),
            state
        )
        HotKey("*$c", (*) => SendEvent("{F17}"), state)
        HotKey("*$v", (*) => SendEvent("{F18}"), state)
    }
}

SetValoModeHotkeys(state) {
    for key, cfg in Map(
        "2",  ["{e}",  "{q Down}", "{q Up}"],
        "1",  ["{.}",  "{b Down}", "{b Up}"],
        "q",  ["{=}",  "{g Down}", "{g Up}"],
        "F1", ["{i}",  "{f Down}", "{f Up}"],
        "3",  ["{v}",  "{r Down}", "{r Up}"],
        "z",  ["{c}",  "{n Down}", "{n Up}"]
    ) {
        tapKey := cfg[1], modDown := cfg[2], modUp := cfg[3]
        HotKey(
            "*$" . key,
            ((k, t, md, mu) => (*) => HandleModTapCustom(k, t, md, mu))(key, tapKey, modDown, modUp),
            state
        )
    }

    HotKey("*$s", (*) => SendEvent("{d}"), state)
}

$F17::Send("{WheelUp}")
$F18::Send("{WheelDown}")
$F19::Send("#{Tab}")
