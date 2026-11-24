# Boot rescue

This document describes the boot rescue of the InayOS desktop profile: the `boot-rescue` module, `crates/modules/src/boot_rescue.rs`, with its files in `dotfiles/assets/boot-rescue`.

The boot rescue watches each boot until a login screen or a desktop is up. When the boot does not get there, it opens Claude Code on tty9 with the boot's context, so the person can repair the system without typing commands. The person who installs the module runs sudo without a password, which lets Claude repair the system from that console.

## The module

The module needs `base` and `kde-plasma`, which installs and enables `plasmalogin.service`. It comes after `boot-guard` in the profile. Its footprint holds `curl` and `networkmanager`, whose `nm-online` the program runs.

### Install

Install:

1. installs `curl` and `networkmanager`;
2. reads the user's name with `id -un`;
3. checks the sudo rule for that user with `visudo -cqf -`, and fails before writing anything when visudo refuses it, because a sudoers file that sudo cannot parse stops every later sudo;
4. writes the files of `dotfiles/assets/boot-rescue` under `/`, the program with mode 0755 and the unit with mode 0644;
5. writes `/etc/inayos-boot-rescue.conf`, which holds `RESCUE_USER=<user>`, with mode 0644;
6. writes `/etc/sudoers.d/20-inayos-boot-rescue`, which holds `<user> ALL=(ALL:ALL) NOPASSWD: ALL`, with mode 0440;
7. enables `inayos-boot-rescue.service`. It does not start it, because the boot it watches is already over.

Every write is recorded, so `remove` deletes the files and disables the service.

### Verify

`verify` fails when `curl` or `networkmanager` is missing, when a file of `dotfiles/assets/boot-rescue` differs from the repository copy, when the configuration or the sudo rule does not name the current user, or when the service is not enabled.

## The service

`/etc/systemd/system/inayos-boot-rescue.service` runs `/usr/local/libexec/inayos-boot-rescue watch` as root. It has no default dependencies and is wanted by `sysinit.target`, `emergency.target` and `rescue.target`, so it starts at the beginning of the boot and also runs when the boot falls into emergency or rescue mode. `IgnoreOnIsolate=yes` keeps it running when systemd isolates one of those targets.

The program checks the boot every two seconds:

- When the active session of `seat0` is a Wayland or X11 session, the login screen or a desktop is up, and the program ends.
- When `emergency.target` or `rescue.target` is active, it opens the console at once.
- When `plasmalogin.service` has failed, it restarts it.
- While `plasmalogin.service` is activating, reloading or deactivating, which a restart goes through, it waits for the next check.
- When `plasmalogin.service` has been active for 30 seconds without such a session, it restarts it. The login screen can run without a session when its greeter exits right after it starts.
- When no job is left and `plasmalogin.service` is inactive, nothing will start the login screen any more, so it opens the console. The program reads the jobs before the state of `plasmalogin.service`, so a login screen that starts between the two reads counts as started.
- When jobs still wait and `plasmalogin.service` is inactive 120 seconds after the program started, or after its last restart of `plasmalogin.service`, the boot is stuck, so it opens the console. The program starts when the initramfs hands over, so the time spent in the initramfs, such as typing a recovery key, does not count.

After three restarts of `plasmalogin.service`, the next failure opens the console instead of a fourth restart. The program writes each restart to the journal.

`inayos-boot-rescue rescue [REASON]` opens the console at once. It tests the console on a running system: `sudo systemd-run --unit=inayos-boot-rescue-test /usr/local/libexec/inayos-boot-rescue rescue test`, then `sudo pkill -t tty9` closes it. `runuser` opens a logind session for the console's programs, so stopping the unit leaves them running.

Once open, the console stays open, also when a login screen comes up later.

## The console

The console opens on tty9 and switches the screen to it. It gives up the switch after 5 seconds, because a display server that hangs while it holds its console can refuse it.

It first writes `/run/inayos-boot-rescue/context.md`. The file holds the reason, the restarts already done, `systemctl is-system-running`, the failed units, the waiting jobs, the sessions, the journal of `plasmalogin.service` and `bootvine.service` for this boot, the warnings and errors of this boot, `bootvine health`, the latest package changes from `/var/log/pacman.log`, and the kernel command line.

The console runs as `RESCUE_USER` from `/etc/inayos-boot-rescue.conf`, the user the sudo rule names. When that user does not exist, the console runs `login`.

In rescue mode, which does not start NetworkManager, it starts NetworkManager without waiting for the start. In emergency mode it leaves NetworkManager alone, because starting it pulls in `sysinit.target`, which ends emergency mode and retries the failed mounts. It then waits up to 60 seconds for the network. When `~/.local/bin/claude` exists and `https://api.anthropic.com` answers, it starts Claude Code with `--dangerously-skip-permissions` and a prompt that gives the reason and points to the context file. Claude runs in `~/.local/state/inayos-boot-rescue`, which `claude-config-helper trust` marks as trusted when the helper exists, so the person's home stays untrusted.

When Claude cannot run, or after it ends, the console prints the start of the context and opens the user's login shell. In emergency mode the network is usually down, so the console usually opens the shell. When that shell ends, the console tries Claude again.

The console cannot help with a failure before the system's own root file system starts, such as a missing root disk or a broken initramfs, because the program lives on that file system. The boot guard's "Previous system" entry in rEFInd covers those failures where the boot guard runs (see `docs/boot-guard.md`).
