libinput:register({1})

local multiplier = 4

libinput:connect("new-evdev-device", function(device)
    local usages = device:usages()
    if not usages[evdev.REL_X] or not usages[evdev.REL_Y] then
        return
    end

    device:connect("evdev-frame", function(_, frame)
        local changed = false

        for _, event in ipairs(frame) do
            if event.usage == evdev.REL_X or event.usage == evdev.REL_Y then
                event.value = event.value * multiplier
                changed = true
            end
        end

        if changed then
            return frame
        end
    end)
end)
