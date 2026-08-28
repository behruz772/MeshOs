#!/usr/bin/env python3

import json
import platform
import socket
import subprocess
import sys
from pathlib import Path

import psutil

from PyQt6.QtCore import QDateTime, QProcess, Qt, QTimer
from PyQt6.QtGui import QFont
from PyQt6.QtWidgets import (
    QApplication,
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QListWidget,
    QMainWindow,
    QPushButton,
    QStackedWidget,
    QVBoxLayout,
    QWidget,
)

ACCENT = "#39C6FF"
ACCENT2 = "#8B5CFF"
BG = "#070B12"
PANEL = "#0C1420"
CARD = "#101B2A"
CARD2 = "#132238"
TEXT = "#F4F8FF"
MUTED = "#8EA1B8"
GREEN = "#35E29A"
RED = "#FF6377"
BORDER = "#1E3048"


class MeshShell(QMainWindow):

    def __init__(self):
        super().__init__()

        self.proc = None

        self.setWindowTitle("MeshOS")
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint
        )
        self.showFullScreen()

        self.build_ui()
        self.refresh()

        self.timer = QTimer(self)
        self.timer.timeout.connect(self.refresh)
        self.timer.start(2000)

    def build_ui(self):

        self.setStyleSheet(f"""
            QWidget {{
                background: {BG};
                color: {TEXT};
                font-family: "Noto Sans";
            }}

            QLineEdit {{
                background: #0D1724;
                border: 1px solid {BORDER};
                border-radius: 12px;
                padding: 12px 16px;
                color: {TEXT};
                font-size: 15px;
            }}

            QLineEdit:focus {{
                border: 1px solid {ACCENT};
            }}

            QPushButton {{
                background: {CARD};
                border: 1px solid {BORDER};
                border-radius: 12px;
                padding: 11px 14px;
                color: {TEXT};
                text-align: left;
            }}

            QPushButton:hover {{
                background: {CARD2};
                border: 1px solid {ACCENT};
            }}

            QListWidget {{
                background: {PANEL};
                border: 0;
                border-radius: 14px;
                padding: 8px;
            }}
        """)

        root = QWidget()
        root_layout = QHBoxLayout(root)
        root_layout.setContentsMargins(18, 18, 18, 18)
        root_layout.setSpacing(16)

        rail = QFrame()
        rail.setFixedWidth(88)
        rail.setStyleSheet(f"""
            QFrame {{
                background: {PANEL};
                border: 1px solid {BORDER};
                border-radius: 22px;
            }}
        """)

        rail_layout = QVBoxLayout(rail)
        rail_layout.setContentsMargins(10, 14, 10, 14)

        logo = QLabel("M")
        logo.setAlignment(Qt.AlignmentFlag.AlignCenter)
        logo.setStyleSheet(f"""
            QLabel {{
                background: {ACCENT2};
                border-radius: 18px;
                color: white;
                font-size: 28px;
                font-weight: 900;
                min-height: 54px;
                max-height: 54px;
            }}
        """)

        rail_layout.addWidget(logo)
        rail_layout.addSpacing(20)

        for icon, name in [
            ("⌂", "Home"),
            ("▣", "Files"),
            ("◈", "Mesh"),
            ("⇅", "Transfers"),
            ("◉", "System"),
            ("⚙", "Settings"),
        ]:
            btn = QPushButton(f"{icon}\n{name}")
            btn.setFixedHeight(58)
            btn.setStyleSheet("""
                QPushButton {
                    border: 0;
                    background: transparent;
                    text-align: center;
                }
                QPushButton:hover {
                    background: #142238;
                }
            """)
            btn.clicked.connect(
                lambda checked=False, n=name:
                    self.go_to(n)
            )
            rail_layout.addWidget(btn)

        rail_layout.addStretch()

        exit_btn = QPushButton("⇱\nExit")
        exit_btn.clicked.connect(self.close)
        exit_btn.setStyleSheet("""
            QPushButton {
                border: 0;
                background: #20151B;
                text-align: center;
            }
        """)
        rail_layout.addWidget(exit_btn)

        main = QWidget()
        main_layout = QVBoxLayout(main)

        top = QHBoxLayout()

        brand = QLabel("MeshOS")
        brand.setStyleSheet(
            f"font-size:22px;font-weight:900;color:{ACCENT};"
        )

        self.search = QLineEdit()
        self.search.setPlaceholderText(
            "Search apps, files, devices, settings..."
        )
        self.search.returnPressed.connect(
            self.search_action
        )

        self.clock = QLabel()
        self.clock.setStyleSheet(
            f"color:{MUTED};font-size:13px;"
        )

        top.addWidget(brand)
        top.addSpacing(30)
        top.addWidget(self.search, 1)
        top.addSpacing(18)
        top.addWidget(self.clock)

        main_layout.addLayout(top)
        main_layout.addSpacing(18)

        self.stack = QStackedWidget()
        main_layout.addWidget(self.stack, 1)

        self.home_page = self.make_home()
        self.mesh_page = self.make_mesh()
        self.files_page = self.make_files()
        self.transfer_page = self.make_transfers()
        self.system_page = self.make_system()
        self.settings_page = self.make_settings()

        for page in [
            self.home_page,
            self.files_page,
            self.mesh_page,
            self.transfer_page,
            self.system_page,
            self.settings_page,
        ]:
            self.stack.addWidget(page)

        root_layout.addWidget(rail)
        root_layout.addWidget(main, 1)

        self.setCentralWidget(root)

    def card(self, title, value, subtitle):

        box = QFrame()
        box.setStyleSheet(f"""
            QFrame {{
                background:{CARD};
                border:1px solid {BORDER};
                border-radius:18px;
            }}
        """)

        layout = QVBoxLayout(box)

        a = QLabel(title)
        a.setStyleSheet(
            f"color:{MUTED};font-size:12px;"
        )

        b = QLabel(value)
        b.setStyleSheet(
            f"color:{TEXT};font-size:25px;font-weight:900;"
        )

        c = QLabel(subtitle)
        c.setStyleSheet(
            f"color:{MUTED};font-size:12px;"
        )

        layout.addWidget(a)
        layout.addWidget(b)
        layout.addWidget(c)

        return box

    def action(self, text, callback):

        b = QPushButton(text)
        b.setMinimumHeight(48)
        b.clicked.connect(callback)
        return b

    def launch(self, command, args=None):

        self.hide()

        self.proc = QProcess(self)

        self.proc.finished.connect(
            self.app_finished
        )

        self.proc.start(
            command,
            args or []
        )

    def app_finished(self):
        self.proc = None
        self.showFullScreen()
        self.raise_()
        self.activateWindow()

    def make_home(self):

        page = QWidget()
        layout = QVBoxLayout(page)

        hero = QFrame()
        hero.setStyleSheet(f"""
            QFrame {{
                border-radius:24px;
                border:1px solid #27415F;
                background:#111E31;
            }}
        """)

        hero_layout = QVBoxLayout(hero)

        hello = QLabel(
            "Good afternoon, Behruz"
        )
        hello.setStyleSheet(
            "font-size:31px;font-weight:900;"
        )

        sub = QLabel(
            "Your devices. Your files. Your system."
        )
        sub.setStyleSheet(
            f"color:{MUTED};font-size:15px;"
        )

        hero_layout.addWidget(hello)
        hero_layout.addWidget(sub)
        hero_layout.addStretch()

        row = QHBoxLayout()

        row.addWidget(
            self.action(
                "▣  Files",
                lambda: self.go_to("Files")
            )
        )

        row.addWidget(
            self.action(
                "◈  Mesh",
                lambda: self.go_to("Mesh")
            )
        )

        row.addWidget(
            self.action(
                "⌘  Terminal",
                lambda: self.launch("konsole")
            )
        )

        row.addWidget(
            self.action(
                "🌐  Browser",
                lambda: self.launch("firefox-esr")
            )
        )

        hero_layout.addLayout(row)

        layout.addWidget(hero)
        layout.addSpacing(15)

        grid = QGridLayout()

        self.cpu = self.card(
            "CPU",
            "—",
            "Loading"
        )

        self.mem = self.card(
            "Memory",
            "—",
            "Loading"
        )

        self.disk = self.card(
            "Storage",
            "—",
            "Loading"
        )

        self.mesh = self.card(
            "Mesh",
            "—",
            "trusted devices"
        )

        grid.addWidget(self.cpu, 0, 0)
        grid.addWidget(self.mem, 0, 1)
        grid.addWidget(self.disk, 0, 2)
        grid.addWidget(self.mesh, 0, 3)

        layout.addLayout(grid)

        layout.addStretch()

        return page

    def make_files(self):

        page = QWidget()
        layout = QVBoxLayout(page)

        title = QLabel("Files")
        title.setStyleSheet(
            "font-size:30px;font-weight:900;"
        )

        layout.addWidget(title)

        for text, target in [
            ("Open File Manager", "/home/meshos"),
            ("Downloads", "/home/meshos/Downloads"),
            ("Documents", "/home/meshos/Documents"),
            ("Mesh Storage", "/var/lib/meshos"),
        ]:
            layout.addWidget(
                self.action(
                    text,
                    lambda p=target:
                        self.launch(
                            "dolphin",
                            [p]
                        )
                )
            )

        layout.addStretch()

        return page

    def make_mesh(self):

        page = QWidget()
        layout = QVBoxLayout(page)

        title = QLabel(
            "Mesh Network"
        )
        title.setStyleSheet(
            "font-size:30px;font-weight:900;"
        )

        layout.addWidget(title)

        self.mesh_info = QLabel()
        self.mesh_info.setStyleSheet(
            f"color:{MUTED};"
        )

        layout.addWidget(
            self.mesh_info
        )

        self.devices = QListWidget()
        layout.addWidget(
            self.devices,
            1
        )

        layout.addWidget(
            self.action(
                "Open Mesh Status",
                lambda:
                    self.launch(
                        "konsole",
                        [
                            "-e",
                            "bash",
                            "-lc",
                            "mesh status; read -p 'Press Enter...'"
                        ]
                    )
            )
        )

        return page

    def make_transfers(self):

        page = QWidget()
        layout = QVBoxLayout(page)

        title = QLabel(
            "Transfers"
        )
        title.setStyleSheet(
            "font-size:30px;font-weight:900;"
        )

        layout.addWidget(title)

        self.transfer_list = QListWidget()
        layout.addWidget(
            self.transfer_list,
            1
        )

        return page

    def make_system(self):

        page = QWidget()
        layout = QVBoxLayout(page)

        title = QLabel(
            "System Center"
        )
        title.setStyleSheet(
            "font-size:30px;font-weight:900;"
        )

        layout.addWidget(title)

        layout.addWidget(
            self.action(
                "MeshOS Control Center",
                lambda:
                    self.launch(
                        "/usr/local/bin/meshos-control-center"
                    )
            )
        )

        layout.addWidget(
            self.action(
                "System Settings",
                lambda:
                    self.launch(
                        "systemsettings"
                    )
            )
        )

        layout.addWidget(
            self.action(
                "System Information",
                lambda:
                    self.launch(
                        "kinfocenter"
                    )
            )
        )

        layout.addWidget(
            self.action(
                "System Monitor",
                lambda:
                    self.launch(
                        "plasma-systemmonitor"
                    )
            )
        )

        layout.addStretch()

        return page

    def make_settings(self):

        page = QWidget()
        layout = QVBoxLayout(page)

        title = QLabel(
            "MeshOS Settings"
        )
        title.setStyleSheet(
            "font-size:30px;font-weight:900;"
        )

        layout.addWidget(title)

        for name, desc in [
            (
                "Appearance",
                "Themes, colors and desktop behavior"
            ),
            (
                "Network",
                "Wi-Fi, Ethernet, VPN and Mesh"
            ),
            (
                "Devices",
                "Bluetooth and trusted devices"
            ),
            (
                "Power",
                "Power and battery management"
            ),
            (
                "Security",
                "Permissions, firewall and identity"
            ),
        ]:
            box = QFrame()
            box.setStyleSheet(
                f"""
                QFrame {{
                    background:{CARD};
                    border:1px solid {BORDER};
                    border-radius:16px;
                }}
                """
            )

            bl = QVBoxLayout(box)

            a = QLabel(name)
            a.setStyleSheet(
                "font-size:16px;font-weight:800;"
            )

            b = QLabel(desc)
            b.setStyleSheet(
                f"color:{MUTED};"
            )

            bl.addWidget(a)
            bl.addWidget(b)

            box.mousePressEvent = (
                lambda event:
                    self.launch("systemsettings")
            )

            layout.addWidget(box)

        layout.addStretch()

        return page

    def go_to(self, name):

        mapping = {
            "Home": 0,
            "Files": 1,
            "Mesh": 2,
            "Transfers": 3,
            "System": 4,
            "Settings": 5,
        }

        if name in mapping:
            self.stack.setCurrentIndex(
                mapping[name]
            )

    def search_action(self):

        q = self.search.text().strip().lower()

        if q in ("files", "file", "file manager"):
            self.go_to("Files")
        elif q in ("mesh", "devices", "network"):
            self.go_to("Mesh")
        elif q in ("transfers", "transfer"):
            self.go_to("Transfers")
        elif q in ("system", "monitor", "system monitor"):
            self.go_to("System")
        elif q in ("settings", "preferences"):
            self.go_to("Settings")
        elif q in ("terminal", "konsole"):
            self.launch("konsole")
        elif q in ("browser", "firefox"):
            self.launch("firefox-esr")

        self.search.clear()

    def trusted_devices(self):

        p = Path(
            "/var/lib/meshos/.meshos_trusted_devices.json"
        )

        if not p.exists():
            return []

        try:
            return json.loads(
                p.read_text()
            )
        except Exception:
            return []

    def refresh(self):

        cpu = psutil.cpu_percent()
        mem = psutil.virtual_memory().percent
        disk = psutil.disk_usage("/").percent

        self.cpu.findChildren(QLabel)[1].setText(
            f"{cpu:.0f}%"
        )

        self.mem.findChildren(QLabel)[1].setText(
            f"{mem:.0f}%"
        )

        self.disk.findChildren(QLabel)[1].setText(
            f"{disk:.0f}%"
        )

        devices = self.trusted_devices()

        self.mesh.findChildren(QLabel)[1].setText(
            str(len(devices))
        )

        self.clock.setText(
            QDateTime.currentDateTime()
            .toString(
                "ddd  dd MMM  HH:mm"
            )
        )

        self.mesh_info.setText(
            f"{len(devices)} trusted MeshOS device(s)"
        )

        self.devices.clear()

        for device in devices:
            self.devices.addItem(
                f"●  {device.get('name', 'MeshOS Device')}   "
                f"{device.get('address', '')}"
            )

        self.transfer_list.clear()

        transfer_file = Path(
            "/var/lib/meshos/transfers.json"
        )

        if transfer_file.exists():
            try:
                data = json.loads(
                    transfer_file.read_text()
                )

                for item in data[-20:]:
                    self.transfer_list.addItem(
                        f"✓  {item.get('file', 'file')}  "
                        f"{item.get('bytes', 0)} bytes"
                    )
            except Exception:
                pass

        if self.transfer_list.count() == 0:
            self.transfer_list.addItem(
                "No transfers yet."
            )


if __name__ == "__main__":

    app = QApplication(sys.argv)

    app.setApplicationName("MeshOS")

    app.setFont(
        QFont("Noto Sans", 10)
    )

    window = MeshShell()
    window.showFullScreen()

    sys.exit(app.exec())
