libinput:register({1})

-- Super + Ctrl + left click becomes a plain right click. Held modifiers are
-- released before the right button goes down and pressed again after it goes
-- up, so applications see no modifiers on the click itself.

local modifier_kinds = {
    [evdev.KEY_LEFTMETA] = "super",
    [evdev.KEY_RIGHTMETA] = "super",
    [evdev.KEY_LEFTCTRL] = "ctrl",
    [evdev.KEY_RIGHTCTRL] = "ctrl",
}

-- Each keyboard records the modifiers physically held on it, and those the
-- plugin reported released although they are still held.
local keyboards = {}

local function kind_held(kind)
    for keyboard in pairs(keyboards) do
        for usage in pairs(keyboard.held) do
            if modifier_kinds[usage] == kind then
                return true
            end
        end
    end
    return false
end

-- Sends one frame per keyboard for the modifiers of one kind. Releases go
-- before the current frame, presses after it.
local function send_modifiers(kind, value)
    for keyboard in pairs(keyboards) do
        local events = {}
        for usage in pairs(keyboard.held) do
            if modifier_kinds[usage] == kind
                and (value == 0) ~= (keyboard.released[usage] == true) then
                events[#events + 1] = { usage = usage, value = value }
                keyboard.released[usage] = value == 0 or nil
            end
        end
        if #events > 0 then
            if value == 0 then
                keyboard.device:prepend_frame(events)
            else
                keyboard.device:append_frame(events)
            end
        end
    end
end

libinput:connect("new-evdev-device", function(device)
    local usages = device:usages()
    local is_keyboard = false
    for usage in pairs(modifier_kinds) do
        is_keyboard = is_keyboard or usages[usage] == true
    end
    -- Clickpads have no right button of their own, so they are left alone.
    local is_pointer = usages[evdev.BTN_LEFT] and usages[evdev.BTN_RIGHT]
    if not is_keyboard and not is_pointer then
        return
    end

    local keyboard = { device = device, held = {}, released = {} }
    if is_keyboard then
        keyboards[keyboard] = true
    end
    local converting = false

    device:connect("evdev-frame", function(_, frame)
        local kept = {}
        local changed = false

        for _, event in ipairs(frame) do
            local keep = true

            if is_keyboard and modifier_kinds[event.usage] then
                local usage = event.usage
                if event.value == 1 then
                    keyboard.held[usage] = true
                elseif event.value == 0 then
                    keyboard.held[usage] = nil
                    if keyboard.released[usage] then
                        keyboard.released[usage] = nil
                        keep = false
                    end
                elseif keyboard.released[usage] then
                    keep = false
                end
            elseif is_pointer and event.usage == evdev.BTN_LEFT then
                if event.value == 1 and not converting
                    and kind_held("super") and kind_held("ctrl") then
                    send_modifiers("super", 0)
                    send_modifiers("ctrl", 0)
                    converting = true
                end
                if converting then
                    event.usage = evdev.BTN_RIGHT
                    changed = true
                    if event.value == 0 then
                        send_modifiers("ctrl", 1)
                        send_modifiers("super", 1)
                        converting = false
                    end
                end
            end

            if keep then
                kept[#kept + 1] = event
            else
                changed = true
            end
        end

        if changed then
            return kept
        end
    end)

    device:connect("device-removed", function()
        keyboards[keyboard] = nil
    end)
end)
