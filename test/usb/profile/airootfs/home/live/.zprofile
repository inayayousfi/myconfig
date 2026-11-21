# getty logs live in on the first console, which starts the Sway session.
if [[ -z $WAYLAND_DISPLAY && $(tty) == /dev/tty1 ]]; then
  # Without a GPU driver, as in a VM, wlroots must accept Mesa's software renderer.
  export WLR_RENDERER_ALLOW_SOFTWARE=1
  # Virtual GPUs draw hardware cursors in the wrong place or not at all.
  export WLR_NO_HARDWARE_CURSORS=1
  exec systemd-cat --identifier=sway sway
fi
