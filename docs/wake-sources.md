# Wake sources

This document describes the `wake-sources` module of the CachyOS profile, `crates/modules/src/wake_sources.rs`, and its file in `dotfiles/assets/wake-sources`.

The module keeps a sleeping laptop asleep until someone means to wake it. Before each sleep, it leaves only keyboards, the power button and the lid able to wake the system. Without it, a wireless mouse receiver, a USB audio dongle, the charger or the network card can wake a laptop whose lid is closed. Plasma then puts it back to sleep about ten seconds later, and the cycle repeats.

## The hook

`/usr/lib/systemd/system-sleep/inayos-wake-sources` is a Bash program. systemd-sleep runs every program in that folder with `pre` before the system sleeps and with `post` after it wakes. The hook acts only on `pre`. It runs at every sleep rather than once at boot, so it also covers devices plugged in later and drivers that turn their wake back on.

It walks every `power/wakeup` file under `/sys/devices`. A device whose file is empty cannot wake the system, and the hook leaves it alone. For the others:

- The built-in keyboard (driver `atkbd`), the lid and the power button (drivers `acpi-button`, `button` and `gpio-keys`) keep their setting.
- USB controllers, PCI devices of class `0x0c03`, keep their setting, because the wake of every USB device below them passes through them. Other PCI devices, such as PCIe ports and the network card, lose their wake.
- USB root hubs keep their setting. A root hub's own wake is for plugging and unplugging, which is off by default.
- A USB device may wake the system when one of its input devices is a keyboard and none is a mouse, as udev's `input_id` classifies them (`ID_INPUT_KEYBOARD` and `ID_INPUT_MOUSE`, read with `udevadm info`). Gaming mouse receivers present a keyboard too, and their mouse excludes them. A keyboard that also presents a mouse, such as a QMK keyboard with mouse keys, cannot wake the system, and the power button wakes it instead.
- A USB hub (device class `09`) may wake the system when such a keyboard sits below it, because the keyboard's wake passes through the hub.
- Every other device loses its wake: touchpads, the battery, the charger and the USB-C power supplies, and the real-time clock with its alarm timer.

It names no hardware, so a new keyboard or mouse needs no change. It prints each change it makes, which systemd-sleep writes to the journal (`journalctl -u systemd-suspend`). The environment variable `INAYOS_WAKE_SYSFS` replaces `/sys` for the tests.

## The module

The module needs `base`, which provides Bash, `find` and udev. It comes after `cachyos-setup` in the profile.

Install writes the hook with mode 0755, as a recorded write, so `remove` deletes it. Nothing runs at install: the hook takes effect at the next sleep. Wake settings that the hook turned off come back at the next boot after a remove.

`verify` fails when the hook differs from the repository copy.
