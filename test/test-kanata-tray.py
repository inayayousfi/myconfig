#!/usr/bin/env python3

import importlib.machinery
import importlib.util
import json
import pathlib
import queue
import socket
import threading
import unittest

TRAY_PATH = (
    pathlib.Path(__file__).parent.parent
    / "dotfiles"
    / "kanata-kde"
    / ".local"
    / "bin"
    / "myconfig-kanata-tray"
)

loader = importlib.machinery.SourceFileLoader("myconfig_kanata_tray", str(TRAY_PATH))
spec = importlib.util.spec_from_loader(loader.name, loader)
tray_module = importlib.util.module_from_spec(spec)
loader.exec_module(tray_module)


class FakeKanataServer:
    def __init__(self, port=0):
        self.listener = socket.socket()
        self.listener.bind(("127.0.0.1", port))
        self.listener.listen(1)
        self.port = self.listener.getsockname()[1]
        self.received = queue.Queue()
        self.connected = threading.Event()
        self._connection = None
        self._thread = threading.Thread(target=self._run, daemon=True)

    def start(self):
        self._thread.start()

    def send(self, message):
        payload = json.dumps(message, separators=(",", ":")) + "\n"
        self._connection.sendall(payload.encode())

    def close(self):
        if self._connection is not None:
            self._connection.close()
        self.listener.close()
        self._thread.join(timeout=2)

    def _run(self):
        try:
            self._connection, _address = self.listener.accept()
            self.connected.set()
            with self._connection.makefile() as stream:
                for line in stream:
                    self.received.put(json.loads(line))
        except OSError:
            pass


class KanataClientTests(unittest.TestCase):
    def setUp(self):
        self.server = FakeKanataServer()
        self.server.start()
        self.client = tray_module.KanataClient(port=self.server.port)
        self.client.start()
        self.assertTrue(self.server.connected.wait(2))

    def tearDown(self):
        self.client.stop()
        self.server.close()

    def wait_for_event(self, expected_kind):
        while True:
            kind, value = self.client.events.get(timeout=2)
            if kind == expected_kind:
                return value

    def test_queries_and_changes_the_active_layer(self):
        self.assertEqual(
            self.server.received.get(timeout=2),
            {"RequestCurrentLayerName": {}},
        )
        self.server.send({"CurrentLayerName": {"name": "off"}})
        self.assertEqual(
            self.wait_for_event("message"),
            {"CurrentLayerName": {"name": "off"}},
        )

        self.client.change_layer("valo")
        self.assertEqual(
            self.server.received.get(timeout=2),
            {"ChangeLayer": {"new": "valo"}},
        )
        self.server.send({"LayerChange": {"new": "valo"}})
        self.assertEqual(
            self.wait_for_event("message"),
            {"LayerChange": {"new": "valo"}},
        )

    def test_reconnects_when_kanata_starts_later(self):
        self.client.stop()
        self.server.close()

        reservation = socket.socket()
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
        reservation.close()

        self.client = tray_module.KanataClient(port=port)
        self.client.start()
        self.wait_for_event("disconnected")

        self.server = FakeKanataServer(port=port)
        self.server.start()
        self.assertTrue(self.server.connected.wait(3))
        self.assertEqual(
            self.server.received.get(timeout=2),
            {"RequestCurrentLayerName": {}},
        )


if __name__ == "__main__":
    unittest.main()
