# InayOS package repository

This document describes the InayOS pacman repository: its two packages in `packaging/`, how a release builds and signs them, how the signing key is made and kept, the local copy on the USB stick, and the `inayos-repository` module of the InayOS desktop profile, `crates/modules/src/inayos_repository.rs`.

The repository is named `inayos`. It lives in the assets of the latest GitHub release, so `pacman -Syu` updates InayOS with the rest of the system.

## Packages

Both PKGBUILDs build from the working tree they sit in and download nothing. Their version is the workspace version from the root `Cargo.toml`, and their release is `INAYOS_PKGREL`, or 1 when it is unset.

`packaging/inayos` holds:

- `/usr/bin/inayos`, the `inayos-arch-desktop` binary;
- `/usr/bin/inayos-guard`, the boot guard (see `docs/boot-guard.md`);
- `/usr/bin/inayos-news`, the Arch news reader (see `docs/maintenance.md`);
- `/usr/share/libalpm/hooks/zzz-inayos-guard.hook`, the guard's pacman hook, from `packaging/inayos/zzz-inayos-guard.hook`.

Its `build()` runs `cargo build --release --locked` for `x86_64-unknown-linux-musl` on the three packages, in `CARGO_TARGET_DIR` or the repository's `target` folder. When `INAYOS_BINARY_DIR` names a folder that holds `inayos`, `inayos-guard` and `inayos-news`, it packages those files instead and builds nothing.

`packaging/inayos-keyring` holds `inayos.gpg`, `inayos-trusted` and `inayos-revoked` in `/usr/share/pacman/keyrings/`, the layout of Arch's own keyring packages, so `pacman-key --populate inayos` adds the key and signs it locally. Its install script runs that command after every install and upgrade when pacman's keyring exists, as `archlinux-keyring` does, so a new key or a revocation reaches every system with the next update.

The committed `inayos.gpg` is the ASCII-armored public key, and `inayos-trusted` holds its `<fingerprint>:4:` line. Until the key exists, both files hold a placeholder whose text starts with `INAYOS-PLACEHOLDER`. The build of `inayos-keyring` fails with a message while the placeholder is there. `INAYOS_KEYRING_DIR` names a folder with the three files to package instead, which the VM xtask uses for its test key.

## Releases

The `build-packages` job of `.github/workflows/release.yml` runs in an `archlinux:latest` container. It installs `base-devel`, `rust`, `rust-musl` and `musl`, and builds both packages with `makepkg` as an unprivileged user, because makepkg refuses root.

It then imports the private key from the secret `INAYOS_PACKAGE_SIGNING_KEY`, an ASCII-armored key without a passphrase, from the environment into `gpg --import` on standard input, in a temporary GnuPG folder. The job stops when the key's fingerprint is not the one in `packaging/inayos-keyring/inayos-trusted`. It signs each package with a detached `.sig` file, and runs `repo-add --sign --key <fingerprint> inayos.db.tar.zst` with both packages. A release asset cannot be a link, so `inayos.db`, `inayos.files` and their `.sig` files are copies of the archives and their signatures.

The `release` job uploads `inayos.db`, `inayos.db.tar.zst`, `inayos.files`, `inayos.files.tar.zst`, the `.sig` file of each, and both packages with their `.sig` files next to the installers. GitHub sends `https://github.com/inayayousfi/myconfig/releases/latest/download/<name>` to the file of that name in the latest release, which is the repository's `Server`.

While the committed keyring is the placeholder, the job fails, and with it the release.

## The signing key

The private key lives in the GitHub Actions secret `INAYOS_PACKAGE_SIGNING_KEY`, with a copy in the user's 1Password vault. It is made by the user in their own terminal, never by an agent, and it never passes through an agent's conversation. With a temporary GnuPG folder in memory:

```bash
export GNUPGHOME=$(mktemp -d -p /dev/shm)
gpg --batch --passphrase '' --quick-gen-key 'InayOS package signing' ed25519 sign never
fingerprint=$(gpg --with-colons --list-secret-keys | awk -F: '$1 == "fpr" { print $10; exit }')
gpg --armor --export-secret-keys "$fingerprint" |
    op document create - --title 'InayOS package signing key' --file-name inayos-signing.asc
op document get 'InayOS package signing key' |
    gh secret set INAYOS_PACKAGE_SIGNING_KEY --repo inayayousfi/myconfig
gpg --armor --export "$fingerprint" > packaging/inayos-keyring/inayos.gpg
printf '%s:4:\n' "$fingerprint" > packaging/inayos-keyring/inayos-trusted
rm -rf "$GNUPGHOME"
```

`op document create -` reads the key from standard input, and `gh secret set` without `--body` reads the secret from standard input, so the private key reaches no file outside the memory folder. The public key and the fingerprint are committed.

A revoked key goes into `inayos-revoked` as a `<fingerprint>` line, and its replacement into `inayos.gpg` and `inayos-trusted`.

## The USB stick

The stick carries a local copy of the repository at `/usr/local/lib/inayos/repo`, which the VM xtask builds on the host from the working tree with `makepkg` and indexes with `repo-add` (see `test/vm/README.md`). The installer installs `inayos` and `inayos-keyring` from it with `pacstrap`, as `docs/installer.md` describes.

That copy is not signed, and only the install's own pacman configuration trusts it, with `SigLevel = Optional TrustAll` for that `file://` server. The stick is the trust root of everything it installs, the installer itself included, so a signature made while the stick is built would add no check. Signing with a throwaway key would also mean every installed system trusts a key whose private part someone other than InayOS could have kept. The new system never reads that configuration: its own `[inayos]` repository is the signed one on GitHub.

## The module

`inayos-repository` comes right after `base` in the profile, before the modules that need the `inayos` package, such as `boot-guard`. It needs `base`.

### Install

1. When `inayos-keyring` is not installed, the module adds the key that the `inayos` binary carries, from the committed `packaging/inayos-keyring`, with `pacman-key --add` and signs it locally with `pacman-key --lsign-key`, so pacman accepts the keyring package. It records an undo that deletes those keys first. A binary built with the placeholder fails here, before any change.
2. It writes `/etc/pacman.d/inayos.conf`, a recorded write, with the `[inayos]` section, `SigLevel = Required` for the database and the packages, and the GitHub `Server`. It writes the section only when the releases are signed with the key that pacman trusts here: the binary carries a real key, not the placeholder, and the installed `inayos-trusted`, when there is one, names that same key. The latest release must also hold `inayos.db`, which `curl` checks. Otherwise the file holds comments only and the module prints why. pacman stops `pacman -Syu` when one sync database cannot be fetched or checked, so a section that the releases cannot serve would stop Arch updates too. A system installed from a VM test build trusts a test key, so its file is the comment-only one. When a package is missing and the repository stays off, the module fails before this step.
3. It adds `Include = /etc/pacman.d/inayos.conf` at the end of `/etc/pacman.conf`, after the Arch repositories, so a name that Arch also uses comes from Arch. pacman has no folder of configuration files, so this is a recorded write of the whole file, which remove puts back.
4. When a package is missing, it runs `pacman -Syu`, so pacman reads the new database, and installs `inayos-keyring` and `inayos`.
5. When pacman's keyring lacks a fingerprint of `/usr/share/pacman/keyrings/inayos-trusted`, it records the undo that deletes those keys and runs `pacman-key --populate inayos`.

In the system that the installer builds, both packages and the key are already in place, so the module writes the two configuration files only. A later `inayos install` writes the `[inayos]` section once the binary and the installed keyring carry the release key.

A failed `pacman -Syu` in step 4 leaves the repository configured. Every later pacman refresh then fails on the `inayos` database until it can be read, or until `inayos remove inayos-repository` puts `pacman.conf` back.

### Verify

`verify` fails when either package is missing, when `/etc/pacman.d/inayos.conf` is neither the comment-only file nor, with the release key trusted, the `[inayos]` section, when `pacman.conf` lacks the include line outside a comment, or when pacman's keyring lacks a key that the installed `inayos-trusted` names.

### Remove

`remove` deletes the keys it added, puts `pacman.conf` back, deletes `/etc/pacman.d/inayos.conf`, and uninstalls the packages that install added, unless another module's footprint still lists them.
