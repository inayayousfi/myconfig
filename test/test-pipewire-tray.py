#!/usr/bin/env python3

import copy
import importlib.machinery
import importlib.util
import os
from pathlib import Path
import shlex
import subprocess
import sys
import unittest
from unittest.mock import MagicMock, patch


sys.dont_write_bytecode = True
TRAY = Path(__file__).resolve().parents[1] / "dotfiles/pipewire/.local/bin/myconfig-pipewire-tray"
loader = importlib.machinery.SourceFileLoader("pipewire_tray", str(TRAY))
spec = importlib.util.spec_from_loader(loader.name, loader)
tray = importlib.util.module_from_spec(spec)
loader.exec_module(tray)


class FakeAudio(tray.AudioRouting):
    def __init__(self):
        self.sinks = [
            {"index": 1, "name": "fiio", "description": "FIIO", "channel_map": "front-left,front-right"},
            {"index": 2, "name": "speaker", "description": "Speaker", "channel_map": "front-left,front-right"},
            {"index": 3, "name": "mono", "channel_map": "mono"},
        ]
        self.streams = [
            {"index": 10, "sink": 1, "properties": {}},
            {"index": 11, "sink": 2, "properties": {}},
        ]
        self.modules = {}
        self.default = "fiio"
        self.next_module = 100
        self.fail_move = False
        self.end_stream = False

    def listing(self, kind):
        return copy.deepcopy(self.sinks if kind == "sinks" else self.streams)

    def command(self, *args):
        if args == ("get-default-sink",):
            return self.default
        if args[:1] == ("set-default-sink",):
            self.default = args[1]
        elif args == ("list", "short", "modules"):
            return "\n".join(f"{index}\tmodule-remap-sink\t{arguments}\t" for index, arguments in self.modules.items())
        elif args[:1] == ("load-module",):
            arguments = " ".join(args[2:])
            props = dict(arg.split("=", 1) for arg in shlex.split(arguments))
            self.next_module += 1
            index = self.next_module
            self.modules[str(index)] = arguments
            self.sinks.append({"index": index, "name": props["sink_name"], "channel_map": "front-left,front-right"})
            master = next(sink["index"] for sink in self.sinks if sink["name"] == props["master"])
            self.streams.append({"index": index, "sink": master, "properties": {"node.name": "output." + props["sink_name"]}})
            return str(index)
        elif args[:1] == ("move-sink-input",):
            stream = next(item for item in self.streams if item["index"] == int(args[1]))
            if self.end_stream:
                self.streams.remove(stream)
                self.end_stream = False
                raise RuntimeError("Stream ended")
            if self.fail_move:
                self.fail_move = False
                raise RuntimeError("Move failed")
            stream["sink"] = next(sink["index"] for sink in self.sinks if sink["name"] == args[2])
        elif args[:1] == ("unload-module",):
            props = dict(arg.split("=", 1) for arg in shlex.split(self.modules.pop(args[1])))
            self.sinks = [sink for sink in self.sinks if sink["name"] != props["sink_name"]]
            self.streams = [stream for stream in self.streams if stream["index"] != int(args[1])]
        else:
            raise AssertionError(args)
        return ""


class AudioRoutingTests(unittest.TestCase):
    def setUp(self):
        self.audio = FakeAudio()

    def test_enable_reverses_map_and_moves_only_selected_playback(self):
        self.audio.enable("fiio")
        module, name = self.audio.corrections()["fiio"]
        self.assertIn("master_channel_map=front-right,front-left", self.audio.modules[module])
        self.assertIn("remix=no", self.audio.modules[module])
        self.assertEqual(self.audio.default, name)
        self.assertEqual(self.audio.streams[0]["sink"], int(module))
        self.assertEqual(self.audio.streams[1]["sink"], 2)
        self.assertEqual(self.audio.streams[2]["sink"], 1)

    def test_disable_restores_default_and_playback(self):
        self.audio.enable("fiio")
        self.audio.disable("fiio")
        self.assertEqual(self.audio.default, "fiio")
        self.assertEqual(self.audio.streams[0]["sink"], 1)
        self.assertEqual(self.audio.corrections(), {})

    def test_nondefault_output_does_not_change_default(self):
        self.audio.enable("speaker")
        self.assertEqual(self.audio.default, "fiio")
        self.audio.disable("speaker")
        self.assertEqual(self.audio.default, "fiio")

    def test_separate_devices_have_independent_corrections(self):
        self.audio.enable("fiio")
        self.audio.enable("speaker")
        self.audio.disable("speaker")
        self.assertEqual(set(self.audio.corrections()), {"fiio"})

    def test_devices_exclude_virtual_corrections_and_mono(self):
        self.audio.enable("fiio")
        self.assertEqual([sink["name"] for sink in self.audio.devices()], ["fiio", "speaker"])

    def test_repeated_enable_and_disable_are_idempotent(self):
        self.audio.enable("fiio")
        self.audio.enable("fiio")
        self.assertEqual(len(self.audio.modules), 1)
        self.audio.disable("fiio")
        self.audio.disable("fiio")

    def test_rejects_missing_or_nonstereo_device(self):
        for master in ("missing", "mono"):
            with self.assertRaises(RuntimeError):
                self.audio.enable(master)
        self.assertEqual(self.audio.modules, {})

    def test_failed_enable_rolls_back_module_and_default(self):
        self.audio.fail_move = True
        with self.assertRaisesRegex(RuntimeError, "Move failed"):
            self.audio.enable("fiio")
        self.assertEqual(self.audio.default, "fiio")
        self.assertEqual(self.audio.modules, {})

    def test_stream_ending_during_move_is_not_a_failure(self):
        self.audio.end_stream = True
        self.audio.enable("fiio")
        self.assertIn("fiio", self.audio.corrections())

    def test_maintenance_routes_new_streams_without_feedback(self):
        self.audio.enable("fiio")
        module, _ = self.audio.corrections()["fiio"]
        self.audio.streams.append({"index": 12, "sink": 1, "properties": {}})
        self.audio.maintain()
        self.assertEqual(self.audio.streams[-1]["sink"], int(module))
        self.assertEqual(self.audio.streams[2]["sink"], 1)

    def test_disconnection_removes_correction(self):
        self.audio.enable("fiio")
        self.audio.sinks = [sink for sink in self.audio.sinks if sink["name"] != "fiio"]
        self.audio.maintain()
        self.assertEqual(self.audio.modules, {})

    def test_disabling_does_not_replace_a_new_default(self):
        self.audio.enable("fiio")
        self.audio.default = "speaker"
        self.audio.disable("fiio")
        self.assertEqual(self.audio.default, "speaker")

    def test_service_restart_is_read_as_correction_off(self):
        self.audio.enable("fiio")
        self.audio.modules.clear()
        self.assertEqual(self.audio.corrections(), {})

    def test_non_ascii_description_fallback(self):
        self.assertEqual(tray.AudioRouting.label({"name": "fiio", "description": "(null)", "properties": {"device.description": "FIIO"}}), "FIIO")

    def test_command_reports_failure_and_timeout(self):
        routing = tray.AudioRouting()
        with patch.object(tray.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "", "Unavailable")):
            with self.assertRaisesRegex(RuntimeError, "Unavailable"):
                routing.command("get-default-sink")
        with patch.object(tray.subprocess, "run", side_effect=subprocess.TimeoutExpired("pactl", 5)):
            with self.assertRaises(subprocess.TimeoutExpired):
                routing.command("get-default-sink")


class TrayMenuTests(unittest.TestCase):
    def test_device_selection_and_swap_checkbox(self):
        os.environ["QT_QPA_PLATFORM"] = "offscreen"
        from PySide6.QtWidgets import QApplication

        audio = FakeAudio()
        icon = MagicMock()

        def inspect_menu():
            menu = icon.setContextMenu.call_args.args[0]
            icon.activated.connect.assert_not_called()
            mono, devices, swap = menu.actions()[:3]
            self.assertEqual(mono.text(), "Mono playback (all outputs)")
            self.assertEqual(devices.menu().title(), "Device for channel correction")
            self.assertTrue(devices.menu().actions()[0].isChecked())
            devices.menu().actions()[1].trigger()
            self.assertEqual(audio.default, "fiio")
            swap.trigger()
            self.assertTrue(swap.isChecked())
            self.assertEqual(set(audio.corrections()), {"speaker"})
            self.assertEqual(audio.default, "fiio")
            swap.trigger()
            self.assertFalse(swap.isChecked())
            self.assertEqual(audio.corrections(), {})
            return 0

        with patch.object(tray, "AudioRouting", return_value=audio), \
                patch("PySide6.QtWidgets.QSystemTrayIcon", return_value=icon), \
                patch.object(QApplication, "exec", side_effect=inspect_menu):
            self.assertEqual(tray.run_tray(), 0)


if __name__ == "__main__":
    unittest.main()
