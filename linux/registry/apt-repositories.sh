#!/usr/bin/env bash

# Vendor apt repositories that package specs name as their source, for example
# tailscale:tailscale. {codename} becomes the release code name of this Ubuntu.
# Arguments: identifier, signing key URL (binary or ASCII-armored), repository
# URI, suite, component.
register_apt_repository tailscale \
    'https://pkgs.tailscale.com/stable/ubuntu/{codename}.noarmor.gpg' \
    'https://pkgs.tailscale.com/stable/ubuntu' '{codename}' main
register_apt_repository github \
    'https://cli.github.com/packages/githubcli-archive-keyring.gpg' \
    'https://cli.github.com/packages' stable main
register_apt_repository docker \
    'https://download.docker.com/linux/ubuntu/gpg' \
    'https://download.docker.com/linux/ubuntu' '{codename}' stable
register_apt_repository yazi \
    'https://yazi-rs.github.io/builds/yazi-keyring.gpg' \
    'https://yazi-rs.github.io/builds/' stable main
