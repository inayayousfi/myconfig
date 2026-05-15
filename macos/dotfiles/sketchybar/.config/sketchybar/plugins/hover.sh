#!/bin/bash

HOVER_COLOR=0xff8fa354

case "$SENDER" in
  "mouse.entered")
    sketchybar --set "$NAME" background.drawing=on background.color=$HOVER_COLOR
    ;;
  "mouse.exited" | "mouse.exited.global")
    sketchybar --set "$NAME" background.drawing=off
    ;;
esac
