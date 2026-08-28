#!/usr/bin/env python3
"""MeshOS Control Center: live view of the installed mesh service."""

import json
import subprocess
import sys
from pathlib import Path

from PyQt6.QtCore import Qt
from PyQt6.QtGui import QFont
from PyQt6.QtWidgets import (
    QApplication, QFrame, QHBoxLayout, QLabel, QListWidget, QMainWindow,
    QPushButton, QStackedWidget, QTextEdit, QVBoxLayout, QWidget,
)

ACCENT = "#39C6FF"
BG = "#070B12"
PANEL = "#0C1420"
CARD = "#101B2A"
TEXT = "#F4F8FF"
MUTED = "#8EA1B8"
GREEN = "#35E29A"
RED = "#FF6377"
BORDER = "#1E3048"
DATA_DIR = Path("/var/lib/meshos")
TRUSTED_DEVICES = DATA_DIR / ".meshos_trusted_devices.json"
TRANSFERS = DATA_DIR / "transfers.json"


class ControlCenter(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("MeshOS Control Center")
        self.resize(1120, 720)
        self.setMinimumSize(860, 560)
        self._build_ui()
        self.refresh_all()

    def _build_ui(self):
        self.setStyleSheet(f"""
            QWidget {{ background: {BG}; color: {TEXT}; font-family: 'Noto Sans'; }}
            QPushButton {{ background: {CARD}; border: 1px solid {BORDER}; border-radius: 10px;
                           padding: 10px 14px; text-align: left; }}
            QPushButton:hover {{ border-color: {ACCENT}; background: #132238; }}
            QListWidget, QTextEdit {{ background: {PANEL}; border: 1px solid {BORDER};
                                      border-radius: 12px; padding: 8px; }}
        """)
        root = QWidget()
        layout = QHBoxLayout(root)
        layout.setContentsMargins(18, 18, 18, 18)
        layout.setSpacing(16)

        rail = QFrame()
        rail.setFixedWidth(172)
        rail.setStyleSheet(f"QFrame {{ background: {PANEL}; border: 1px solid {BORDER}; border-radius: 18px; }}")
        rail_layout = QVBoxLayout(rail)
        brand = QLabel("M\nMeshOS")
        brand.setAlignment(Qt.AlignmentFlag.AlignCenter)
        brand.setStyleSheet(f"color: {ACCENT}; font-size: 22px; font-weight: 900;")
        rail_layout.addWidget(brand)
        rail_layout.addSpacing(12)
        self.stack = QStackedWidget()
        for index, name in enumerate(("Mesh", "Devices", "Transfers", "Security", "Diagnostics")):
            button = QPushButton(name)
            button.clicked.connect(lambda checked=False, i=index: self.show_page(i))
            rail_layout.addWidget(button)
        rail_layout.addStretch()
        refresh = QPushButton("↻  Refresh live state")
        refresh.clicked.connect(self.refresh_all)
        rail_layout.addWidget(refresh)

        self.mesh_page = self._mesh_page()
        self.devices_page = self._devices_page()
        self.transfers_page = self._transfers_page()
        self.security_page = self._security_page()
        self.diagnostics_page = self._diagnostics_page()
        for page in (self.mesh_page, self.devices_page, self.transfers_page, self.security_page, self.diagnostics_page):
            self.stack.addWidget(page)
        layout.addWidget(rail)
        layout.addWidget(self.stack, 1)
        self.setCentralWidget(root)

    def _page(self, title, subtitle):
        page = QWidget()
        layout = QVBoxLayout(page)
        title_label = QLabel(title)
        title_label.setStyleSheet("font-size: 30px; font-weight: 900;")
        subtitle_label = QLabel(subtitle)
        subtitle_label.setStyleSheet(f"color: {MUTED}; font-size: 14px;")
        layout.addWidget(title_label)
        layout.addWidget(subtitle_label)
        layout.addSpacing(12)
        return page, layout

    def _mesh_page(self):
        page, layout = self._page("Mesh", "Live MeshOS service and local network state")
        self.mesh_status = QTextEdit()
        self.mesh_status.setReadOnly(True)
        layout.addWidget(self.mesh_status, 1)
        return page

    def _devices_page(self):
        page, layout = self._page("Devices", "Trusted devices known by the MeshOS daemon")
        self.devices = QListWidget()
        layout.addWidget(self.devices, 1)
        return page

    def _transfers_page(self):
        page, layout = self._page("Transfers", "Recorded MeshOS file transfers")
        self.transfers = QListWidget()
        layout.addWidget(self.transfers, 1)
        return page

    def _security_page(self):
        page, layout = self._page("Security", "Trust state from MeshOS pairing records")
        self.security = QTextEdit()
        self.security.setReadOnly(True)
        layout.addWidget(self.security, 1)
        return page

    def _diagnostics_page(self):
        page, layout = self._page("Diagnostics", "Actual meshos-daemon status and recent journal output")
        row = QHBoxLayout()
        restart = QPushButton("Restart Mesh daemon")
        restart.clicked.connect(self.restart_daemon)
        logs = QPushButton("Refresh diagnostics")
        logs.clicked.connect(self.refresh_diagnostics)
        row.addWidget(restart)
        row.addWidget(logs)
        row.addStretch()
        layout.addLayout(row)
        self.diagnostics = QTextEdit()
        self.diagnostics.setReadOnly(True)
        layout.addWidget(self.diagnostics, 1)
        return page

    @staticmethod
    def command(*args):
        try:
            result = subprocess.run(args, capture_output=True, text=True, timeout=8, check=False)
            output = (result.stdout + result.stderr).strip()
            return output or "No output."
        except (OSError, subprocess.TimeoutExpired) as error:
            return f"Unavailable: {error}"

    @staticmethod
    def load_json(path):
        if not path.exists():
            return []
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
            return value if isinstance(value, list) else []
        except (OSError, json.JSONDecodeError):
            return []

    def show_page(self, index):
        self.stack.setCurrentIndex(index)
        if index == 4:
            self.refresh_diagnostics()

    def refresh_all(self):
        self.refresh_mesh()
        self.refresh_devices()
        self.refresh_transfers()
        self.refresh_security()
        if self.stack.currentIndex() == 4:
            self.refresh_diagnostics()

    def refresh_mesh(self):
        self.mesh_status.setPlainText(self.command("/usr/local/bin/mesh", "status"))

    def refresh_devices(self):
        devices = self.load_json(TRUSTED_DEVICES)
        self.devices.clear()
        if not devices:
            self.devices.addItem("No trusted MeshOS devices yet.")
            return
        for device in devices:
            name = device.get("name", "MeshOS Device")
            address = device.get("address", "unknown address")
            device_id = device.get("id", "unknown id")
            self.devices.addItem(f"●  {name}\n   {address}\n   ID: {device_id}")

    def refresh_transfers(self):
        transfers = self.load_json(TRANSFERS)
        self.transfers.clear()
        if not transfers:
            self.transfers.addItem("No recorded MeshOS transfers yet.")
            return
        for item in reversed(transfers[-100:]):
            name = item.get("file", "unnamed file")
            size = item.get("bytes", 0)
            status = item.get("status", "completed")
            self.transfers.addItem(f"{status}  {name}  —  {size} bytes")

    def refresh_security(self):
        devices = self.load_json(TRUSTED_DEVICES)
        lines = [
            "MeshOS trust model",
            "• Discovery advertisements are validated as Ed25519 public keys.",
            "• Pairing establishes an X25519/HKDF encrypted session.",
            "• Trusted device records are held by meshos-daemon.",
            "",
            f"Trusted devices: {len(devices)}",
        ]
        for device in devices:
            key = device.get("public_key", "")
            fingerprint = key[:16] + "…" if len(key) > 16 else (key or "not recorded")
            lines.append(f"• {device.get('name', 'MeshOS Device')} — key {fingerprint}")
        self.security.setPlainText("\n".join(lines))

    def refresh_diagnostics(self):
        active = self.command("systemctl", "is-active", "meshos-daemon")
        enabled = self.command("systemctl", "is-enabled", "meshos-daemon")
        logs = self.command("journalctl", "-u", "meshos-daemon", "--no-pager", "-n", "80")
        color = GREEN if active == "active" else RED
        self.diagnostics.setHtml(
            f"<p><b style='color:{color}'>Service: {active}</b><br>Enabled: {enabled}</p>"
            f"<pre>{logs}</pre>"
        )

    def restart_daemon(self):
        result = self.command("pkexec", "systemctl", "restart", "meshos-daemon")
        self.refresh_diagnostics()
        self.diagnostics.append("\nRestart request:\n" + result)


if __name__ == "__main__":
    app = QApplication(sys.argv)
    app.setApplicationName("MeshOS Control Center")
    app.setFont(QFont("Noto Sans", 10))
    window = ControlCenter()
    window.show()
    sys.exit(app.exec())
