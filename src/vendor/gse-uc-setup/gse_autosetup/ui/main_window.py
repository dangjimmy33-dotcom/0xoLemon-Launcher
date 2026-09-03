from __future__ import annotations

import ctypes
import os
import sys
from datetime import datetime
from pathlib import Path

from PySide6.QtCore import QEasingCurve, QObject, Property, QPropertyAnimation, QThread, Qt, Signal, Slot
from PySide6.QtGui import QColor, QCursor, QDesktopServices, QIcon, QPainter
from PySide6.QtCore import QUrl
from PySide6.QtWidgets import (
    QAbstractButton,
    QApplication,
    QButtonGroup,
    QComboBox,
    QDoubleSpinBox,
    QFileDialog,
    QFrame,
    QGraphicsOpacityEffect,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMainWindow,
    QMessageBox,
    QPlainTextEdit,
    QProgressBar,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QSpinBox,
    QStackedWidget,
    QVBoxLayout,
    QWidget,
)

from ..service import Inputs, SetupService, validate_inputs
from ..core.migrate import MigrateGSEManager
from ..core.resources import ResourceManager
from ..core.tool_config import ToolConfig, default_config_path, load_config, save_config
from ..core.uc_online import UCOnlineResourceManager
from ..core.steamstub import SteamStubManager
from ..core.steamless import SteamlessManager
from .theme import APP_QSS
from .save_manager_tab import SaveManagerTab


def resource_path(relative: str) -> str:
    base = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parents[2]))
    return str(base / relative)


def apply_windows_backdrop(window: QWidget) -> None:
    """Enable native Windows 11 dark titlebar, rounded corners and Mica."""
    if os.name != "nt":
        return
    hwnd = int(window.winId())
    try:
        dark = ctypes.c_int(1)
        for attr in (20, 19):
            if ctypes.windll.dwmapi.DwmSetWindowAttribute(
                hwnd, attr, ctypes.byref(dark), ctypes.sizeof(dark)
            ) == 0:
                break
        # DWMWA_SYSTEMBACKDROP_TYPE=38 / DWMSBT_MAINWINDOW=2 = Mica.
        backdrop = ctypes.c_int(2)
        ctypes.windll.dwmapi.DwmSetWindowAttribute(
            hwnd, 38, ctypes.byref(backdrop), ctypes.sizeof(backdrop)
        )
        # DWMWA_WINDOW_CORNER_PREFERENCE=33 / DWMWCP_ROUND=2.
        corners = ctypes.c_int(2)
        ctypes.windll.dwmapi.DwmSetWindowAttribute(
            hwnd, 33, ctypes.byref(corners), ctypes.sizeof(corners)
        )
    except Exception:
        pass


class ToggleSwitch(QAbstractButton):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setCheckable(True)
        self.setFixedSize(46, 25)
        self.setCursor(QCursor(Qt.PointingHandCursor))
        self._position = 0.0
        self._reduced_motion = False
        self._animation = QPropertyAnimation(self, b"position", self)
        self._animation.setDuration(180)
        self._animation.setEasingCurve(QEasingCurve.OutCubic)
        self.toggled.connect(self._animate)

    def set_reduced_motion(self, value: bool) -> None:
        self._reduced_motion = bool(value)
        self._animation.setDuration(0 if self._reduced_motion else 180)

    def get_position(self) -> float:
        return self._position

    def set_position(self, value: float) -> None:
        self._position = float(value)
        self.update()

    position = Property(float, get_position, set_position)

    def _animate(self, checked: bool) -> None:
        self._animation.stop()
        if self._reduced_motion:
            self.set_position(1.0 if checked else 0.0)
            return
        self._animation.setStartValue(self._position)
        self._animation.setEndValue(1.0 if checked else 0.0)
        self._animation.start()

    def showEvent(self, event):
        self._position = 1.0 if self.isChecked() else 0.0
        super().showEvent(event)

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing, True)
        painter.setPen(Qt.NoPen)
        if not self.isEnabled():
            track, knob = QColor("#26313d"), QColor("#6b7888")
        elif self.isChecked():
            track, knob = QColor("#3b91ff"), QColor("#ffffff")
        else:
            track, knob = QColor("#384654"), QColor("#d7dfe8")
        painter.setBrush(track)
        painter.drawRoundedRect(0, 1, 46, 23, 11.5, 11.5)
        x = 3 + self._position * 21
        painter.setBrush(knob)
        painter.drawEllipse(int(x), 4, 17, 17)


class AnimatedBody(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self._expanded = False
        self._reduced_motion = False
        self.setMaximumHeight(0)
        self.setVisible(False)
        self._animation = QPropertyAnimation(self, b"maximumHeight", self)
        self._animation.setDuration(210)
        self._animation.setEasingCurve(QEasingCurve.OutCubic)

    def set_reduced_motion(self, value: bool) -> None:
        self._reduced_motion = bool(value)
        self._animation.setDuration(0 if self._reduced_motion else 210)

    def set_expanded(self, expanded: bool) -> None:
        expanded = bool(expanded)
        if self._expanded == expanded:
            return
        self._expanded = expanded
        self._animation.stop()
        if expanded:
            self.setVisible(True)
            target = max(190, self.sizeHint().height())
            if self._reduced_motion:
                self.setMaximumHeight(target)
                return
            self._animation.setStartValue(self.maximumHeight())
            self._animation.setEndValue(target)
            self._animation.start()
        else:
            if self._reduced_motion:
                self.setMaximumHeight(0)
                self.setVisible(False)
                return
            self._animation.setStartValue(self.maximumHeight())
            self._animation.setEndValue(0)
            try:
                self._animation.finished.disconnect(self._hide_if_collapsed)
            except Exception:
                pass
            self._animation.finished.connect(self._hide_if_collapsed)
            self._animation.start()

    def _hide_if_collapsed(self):
        try:
            self._animation.finished.disconnect(self._hide_if_collapsed)
        except Exception:
            pass
        if not self._expanded:
            self.setVisible(False)


class SetupWorker(QObject):
    log = Signal(str)
    progress = Signal(int, str)
    finished = Signal(object)
    failed = Signal(str)

    def __init__(self, inputs: Inputs):
        super().__init__()
        self.inputs = inputs

    @Slot()
    def run(self):
        try:
            service = SetupService(log=self.log.emit, progress=self.progress.emit)
            self.finished.emit(service.run(self.inputs))
        except Exception as exc:
            self.failed.emit(str(exc))


class RestoreWorker(QObject):
    log = Signal(str)
    progress = Signal(int, str)
    finished = Signal(object)
    failed = Signal(str)

    def __init__(self, game_folder: Path):
        super().__init__()
        self.game_folder = game_folder

    @Slot()
    def run(self):
        try:
            service = SetupService(log=self.log.emit, progress=self.progress.emit)
            self.finished.emit(service.restore(self.game_folder))
        except Exception as exc:
            self.failed.emit(str(exc))


class ResourceStatusWorker(QObject):
    finished = Signal(dict)
    failed = Signal(str)

    @Slot()
    def run(self):
        try:
            resources = ResourceManager()
            data: dict[str, str] = {}
            try:
                data["gse"] = SetupService().check_latest_release().tag
            except Exception:
                data["gse"] = "check failed"
            try:
                data["uc"] = UCOnlineResourceManager(resources=resources).latest_release().tag
            except Exception:
                data["uc"] = "check failed"
            try:
                data["rune"] = SteamStubManager(resources=resources).latest_release().tag
            except Exception:
                data["rune"] = "check failed"
            data["steamless"] = SteamlessManager(resources=resources).latest_release().tag
            data["migrate"] = "embedded / portable override"
            self.finished.emit(data)
        except Exception as exc:
            self.failed.emit(str(exc))


class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("GSE / UC Setup")
        self.resize(1440, 930)
        self.setMinimumSize(1120, 740)
        icon = resource_path("assets/icon.ico")
        if Path(icon).is_file():
            self.setWindowIcon(QIcon(icon))
        self._thread: QThread | None = None
        self._resource_thread: QThread | None = None
        self._progress_animation: QPropertyAnimation | None = None
        self.config_path = default_config_path()
        self.setStyleSheet(APP_QSS)
        self._build_ui()
        self._load_settings()
        self._refresh_context()
        self._refresh_local_resource_labels()

    def showEvent(self, event):
        super().showEvent(event)
        apply_windows_backdrop(self)

    def closeEvent(self, event):
        self._save_settings(silent=True)
        super().closeEvent(event)

    # ---------- tiny UI helpers ----------
    def _card(self, strong: bool = False):
        card = QFrame()
        card.setObjectName("GlassCardStrong" if strong else "GlassCard")
        layout = QVBoxLayout(card)
        layout.setContentsMargins(22, 20, 22, 20)
        layout.setSpacing(14)
        return card, layout

    def _section_title(self, title: str, subtitle: str = "") -> QWidget:
        wrap = QWidget()
        lay = QVBoxLayout(wrap)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(3)
        t = QLabel(title)
        t.setObjectName("SectionTitle")
        lay.addWidget(t)
        if subtitle:
            s = QLabel(subtitle)
            s.setObjectName("Muted")
            s.setWordWrap(True)
            lay.addWidget(s)
        return wrap

    def _field(self, title: str, control: QWidget, hint: str = "") -> QWidget:
        wrap = QWidget()
        lay = QVBoxLayout(wrap)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(6)
        label = QLabel(title)
        label.setObjectName("FieldLabel")
        lay.addWidget(label)
        lay.addWidget(control)
        if hint:
            note = QLabel(hint)
            note.setObjectName("Muted")
            note.setWordWrap(True)
            lay.addWidget(note)
        return wrap

    def _setting_row(self, title: str, subtitle: str, control: QWidget) -> QWidget:
        row = QWidget()
        lay = QHBoxLayout(row)
        lay.setContentsMargins(0, 2, 0, 2)
        lay.setSpacing(18)
        text = QVBoxLayout()
        text.setSpacing(2)
        name = QLabel(title)
        name.setObjectName("FieldLabel")
        desc = QLabel(subtitle)
        desc.setObjectName("Muted")
        desc.setWordWrap(True)
        text.addWidget(name)
        text.addWidget(desc)
        lay.addLayout(text, 1)
        lay.addWidget(control, 0, Qt.AlignRight | Qt.AlignVCenter)
        return row

    def _segment(self, labels: list[tuple[str, str]], checked: str) -> tuple[QWidget, dict[str, QPushButton]]:
        wrap = QWidget()
        lay = QHBoxLayout(wrap)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(7)
        group = QButtonGroup(wrap)
        group.setExclusive(True)
        buttons: dict[str, QPushButton] = {}
        for text, key in labels:
            b = QPushButton(text)
            b.setObjectName("Segment")
            b.setCheckable(True)
            b.setProperty("value", key)
            group.addButton(b)
            lay.addWidget(b, 1)
            buttons[key] = b
        if checked in buttons:
            buttons[checked].setChecked(True)
        wrap._group = group  # type: ignore[attr-defined]
        return wrap, buttons

    def _current_segment(self, buttons: dict[str, QPushButton], default: str) -> str:
        for key, button in buttons.items():
            if button.isChecked():
                return key
        return default

    # ---------- build ----------
    def _build_ui(self) -> None:
        root = QWidget()
        root.setObjectName("AppRoot")
        self.setCentralWidget(root)
        shell = QVBoxLayout(root)
        shell.setContentsMargins(0, 0, 0, 0)
        shell.setSpacing(0)

        # Header is aligned to the same max-width as the content.
        header_host = QWidget()
        header_host.setObjectName("TopBar")
        hh = QHBoxLayout(header_host)
        hh.setContentsMargins(32, 20, 32, 16)
        header = QWidget()
        header.setMaximumWidth(1240)
        header.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Preferred)
        hl = QHBoxLayout(header)
        hl.setContentsMargins(0, 0, 0, 0)
        title_box = QVBoxLayout()
        title = QLabel("GSE / UC Setup")
        title.setObjectName("AppTitle")
        subtitle = QLabel("Portable Steam integration workspace · GSE single-player/offline/LAN · UC Online2 Spacewar")
        subtitle.setObjectName("AppSubtitle")
        title_box.addWidget(title)
        title_box.addWidget(subtitle)
        hl.addLayout(title_box, 1)
        self.engine_chip = QLabel("GSE")
        self.engine_chip.setObjectName("ChipBlue")
        self.resource_chip = QLabel("Hybrid resources")
        self.resource_chip.setObjectName("ChipNeutral")
        hl.addWidget(self.engine_chip)
        hl.addWidget(self.resource_chip)
        hh.addStretch(1)
        hh.addWidget(header, 8)
        hh.addStretch(1)
        shell.addWidget(header_host)

        # Top-level workspaces. Setup keeps its existing state while the Savegame
        # Manager lives as an independent second tab in the same process.
        nav_host = QWidget()
        nav_layout = QHBoxLayout(nav_host)
        nav_layout.setContentsMargins(32, 10, 32, 8)
        nav_inner = QWidget()
        nav_inner.setMaximumWidth(1240)
        nav_inner.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Preferred)
        nav_row = QHBoxLayout(nav_inner)
        nav_row.setContentsMargins(0, 0, 0, 0)
        nav_row.setSpacing(8)
        self.setup_tab_button = QPushButton("Setup & Emulator")
        self.setup_tab_button.setObjectName("Segment")
        self.setup_tab_button.setCheckable(True)
        self.setup_tab_button.setChecked(True)
        self.save_tab_button = QPushButton("Savegame Manager")
        self.save_tab_button.setObjectName("Segment")
        self.save_tab_button.setCheckable(True)
        self.setup_tab_button.clicked.connect(lambda: self._switch_workspace(0))
        self.save_tab_button.clicked.connect(lambda: self._switch_workspace(1))
        nav_row.addWidget(self.setup_tab_button)
        nav_row.addWidget(self.save_tab_button)
        nav_row.addStretch(1)
        nav_layout.addStretch(1)
        nav_layout.addWidget(nav_inner, 8)
        nav_layout.addStretch(1)
        shell.addWidget(nav_host)

        self.workspace_stack = QStackedWidget()

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        scroll_host = QWidget()
        outer = QHBoxLayout(scroll_host)
        outer.setContentsMargins(30, 8, 30, 24)
        outer.setSpacing(0)
        outer.addStretch(1)
        content = QWidget()
        content.setMaximumWidth(1240)
        content.setMinimumWidth(820)
        content.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Preferred)
        page = QVBoxLayout(content)
        page.setContentsMargins(0, 0, 0, 0)
        page.setSpacing(14)
        outer.addWidget(content, 8)
        outer.addStretch(1)
        scroll.setWidget(scroll_host)
        self.workspace_stack.addWidget(scroll)
        self.save_manager_tab = SaveManagerTab(self)
        self.workspace_stack.addWidget(self.save_manager_tab)
        shell.addWidget(self.workspace_stack, 1)

        # Game card.
        game_card, game = self._card(strong=True)
        game.addWidget(self._section_title("Game", "The API key is required for GSE metadata enrichment; UC Online can run without it."))
        top = QHBoxLayout()
        self.appid = QLineEdit()
        self.appid.setPlaceholderText("Steam AppID")
        self.game_folder = QLineEdit()
        self.game_folder.setPlaceholderText(r"D:\Games\Game")
        browse = QPushButton("Browse")
        browse.clicked.connect(self._browse_game)
        folder_row = QWidget()
        fr = QHBoxLayout(folder_row)
        fr.setContentsMargins(0, 0, 0, 0)
        fr.setSpacing(8)
        fr.addWidget(self.game_folder, 1)
        fr.addWidget(browse)
        top.addWidget(self._field("Real AppID", self.appid), 1)
        top.addWidget(self._field("Game folder", folder_row), 3)
        game.addLayout(top)
        self.api_key = QLineEdit()
        self.api_key.setEchoMode(QLineEdit.Password)
        self.api_key.setPlaceholderText("Steam Web API key")
        self.remember_api = ToggleSwitch()
        api_row = QHBoxLayout()
        api_row.addWidget(self._field("Steam Web API key", self.api_key, "Saved beside the EXE with Windows DPAPI when Remember is enabled."), 1)
        remember = QWidget()
        rl = QVBoxLayout(remember)
        rl.setContentsMargins(12, 19, 0, 0)
        rl.addWidget(self._setting_row("Remember key", "Encrypted for this Windows user.", self.remember_api))
        api_row.addWidget(remember, 1)
        game.addLayout(api_row)
        page.addWidget(game_card)

        # Engine card.
        engine_card, engine = self._card(strong=True)
        engine.addWidget(self._section_title("Engine", "GSE, UC Online2, and RUNE AutoCracker are independent deployment engines; settings are context-sensitive."))
        engine_row = QHBoxLayout()
        self.engine_gse = QPushButton("GSE\nOffline / LAN emulator")
        self.engine_gse.setObjectName("ModeCard")
        self.engine_gse.setCheckable(True)
        self.engine_uc = QPushButton("UC Online\nSteam client + Spacewar")
        self.engine_uc.setObjectName("ModeCard")
        self.engine_uc.setCheckable(True)
        self.engine_rune = QPushButton("RUNE AutoCracker\nRegular / Steak / Steamclient")
        self.engine_rune.setObjectName("ModeCard")
        self.engine_rune.setCheckable(True)
        self.engine_group = QButtonGroup(self)
        self.engine_group.setExclusive(True)
        self.engine_group.addButton(self.engine_gse)
        self.engine_group.addButton(self.engine_uc)
        self.engine_group.addButton(self.engine_rune)
        self.engine_gse.setChecked(True)
        self.engine_gse.clicked.connect(self._refresh_context)
        self.engine_uc.clicked.connect(self._refresh_context)
        self.engine_rune.clicked.connect(self._refresh_context)
        engine_row.addWidget(self.engine_gse, 1)
        engine_row.addWidget(self.engine_uc, 1)
        engine_row.addWidget(self.engine_rune, 1)
        engine.addLayout(engine_row)

        # GSE contextual pane.
        self.gse_pane = QFrame()
        self.gse_pane.setObjectName("InsetCard")
        gp = QVBoxLayout(self.gse_pane)
        gp.setContentsMargins(18, 16, 18, 16)
        gp.setSpacing(12)
        gp.addWidget(self._section_title("GSE deployment", "Regular replaces Steam API; Experimental adds native overlay; ColdClient uses the full steamclient_experimental loader; ColdClient v1 drops steamclient DLLs directly for games that import them (Unity IL2CPP, DX12)."))
        variant_wrap, self.gse_variant_buttons = self._segment(
            [
                ("Regular", "regular"),
                ("Experimental", "experimental"),
                ("ColdClient", "coldclient"),
                ("ColdClient v1", "coldclient_simple"),
            ],
            "regular",
        )
        for b in self.gse_variant_buttons.values():
            b.clicked.connect(self._refresh_context)
        gp.addWidget(variant_wrap)
        network_wrap, self.network_buttons = self._segment(
            [
                ("Single-player", "singleplayer"),
                ("Strict offline", "strict_offline"),
                ("LAN", "lan"),
            ],
            "singleplayer",
        )
        gp.addWidget(self._field(
            "Connectivity",
            network_wrap,
            "Single-player (recommended): GSE networking disabled but Steam reports logged-on. "
            "Strict offline: Steam reports offline. LAN: GSE networking enabled.",
        ))
        engine.addWidget(self.gse_pane)

        # UC contextual pane.
        self.uc_pane = QFrame()
        self.uc_pane.setObjectName("InsetCard")
        up = QVBoxLayout(self.uc_pane)
        up.setContentsMargins(18, 16, 18, 16)
        up.setSpacing(10)
        up.addWidget(self._section_title("UC Online2", "Runs with the real Steam client while spoofing the multiplayer AppID."))
        self.uc_spoof = QLineEdit("480")
        self.uc_spoof.setPlaceholderText("480")
        up.addWidget(self._field("Spoof AppID", self.uc_spoof, "Spacewar 480 is the default. Use another free multiplayer AppID only when the game requires it."))
        self.uc_auto_plugins = ToggleSwitch()
        self.uc_eos = ToggleSwitch()
        self.uc_photon = ToggleSwitch()
        self.uc_playfab = ToggleSwitch()
        self.uc_coherence = ToggleSwitch()
        up.addWidget(self._setting_row("Auto detect", "Scan common backend DLLs automatically.", self.uc_auto_plugins))
        up.addWidget(self._setting_row("EOS", "Deploy Epic Online Services plugin.", self.uc_eos))
        up.addWidget(self._setting_row("Photon", "Deploy Photon networking plugin.", self.uc_photon))
        up.addWidget(self._setting_row("PlayFab", "Deploy Azure PlayFab backend plugin.", self.uc_playfab))
        up.addWidget(self._setting_row("coherence", "Deploy coherence networking plugin.", self.uc_coherence))
        engine.addWidget(self.uc_pane)

        # RUNE contextual pane.
        self.rune_pane = QFrame()
        self.rune_pane.setObjectName("InsetCard")
        rp = QVBoxLayout(self.rune_pane)
        rp.setContentsMargins(18, 16, 18, 16)
        rp.setSpacing(10)
        rp.addWidget(self._section_title("RUNE deployment profile", "Regular replaces Steam API; Steakclient uses winmm proxy loader; Steamclient hooks binary with overlay renderer."))
        rune_wrap, self.rune_profile_buttons = self._segment(
            [
                ("Regular Emu", "regular"),
                ("Steakclient", "steakclient"),
                ("Steamclient", "steamclient"),
            ],
            "regular",
        )
        for b in self.rune_profile_buttons.values():
            b.clicked.connect(self._refresh_context)
        rp.addWidget(rune_wrap)

        rune_fields = QHBoxLayout()
        rune_fields.setSpacing(18)
        self.rune_username = QLineEdit("RUNE")
        self.rune_username.setPlaceholderText("RUNE")
        self.rune_language = QLineEdit("english")
        self.rune_language.setPlaceholderText("english")
        rune_fields.addWidget(self._field("Username", self.rune_username, "Player account name in steam_emu.ini"), 1)
        rune_fields.addWidget(self._field("Language", self.rune_language, "Language code in steam_emu.ini"), 1)
        rp.addLayout(rune_fields)

        self.rune_unlock_all = ToggleSwitch()
        self.rune_lobby = ToggleSwitch()
        self.rune_lobby.setChecked(True)
        self.rune_overlays = ToggleSwitch()
        self.rune_overlays.setChecked(True)
        self.rune_offline = ToggleSwitch()
        rp.addWidget(self._setting_row("Unlock All DLCs", "Set DLCUnlockall=1 in config.", self.rune_unlock_all))
        rp.addWidget(self._setting_row("Lobby Enabled", "Enable Steam lobby support.", self.rune_lobby))
        rp.addWidget(self._setting_row("Overlays", "Enable Steam overlay rendering.", self.rune_overlays))
        rp.addWidget(self._setting_row("Offline Mode", "Force emulator offline flag.", self.rune_offline))
        engine.addWidget(self.rune_pane)
        page.addWidget(engine_card)

        # DRM + configuration in two columns.
        columns = QHBoxLayout()
        columns.setSpacing(14)
        drm_card, drm = self._card()
        drm.addWidget(self._section_title("SteamStub handling", "Steamless edits the EXE only after a successful unpack. Proxy/runtime methods remain explicit choices."))
        self.steamstub_mode = QComboBox()
        self.steamstub_mode.addItem("Auto — Steamless first, never silent proxy fallback", "auto")
        self.steamstub_mode.addItem("Steamless — unpack/patch executable", "steamless")
        self.steamstub_mode.addItem("RUNE SteamStub Patcher — explicit winmm.dll proxy", "rune")
        self.steamstub_mode.addItem("UC Runtime SteamStub — GetStubbedLol", "uc_runtime")
        self.steamstub_mode.addItem("Disabled", "disabled")
        drm.addWidget(self._field("Method", self.steamstub_mode))
        self.drm_hint = QLabel("Auto: Steamless first. If it cannot unpack, no DLL is dropped automatically.")
        self.drm_hint.setObjectName("Hint")
        self.drm_hint.setWordWrap(True)
        drm.addWidget(self.drm_hint)
        columns.addWidget(drm_card, 1)

        config_card, cfg = self._card()
        cfg.addWidget(self._section_title("Identity & saves"))
        self.account_name = QLineEdit("0xoLemon")
        cfg.addWidget(self._field("Account name", self.account_name))
        self.save_mode = QComboBox()
        self.save_mode.addItem("GSE global — %APPDATA%\\GSE Saves", "gse")
        self.save_mode.addItem("Portable — game folder", "portable")
        self.save_mode.addItem("Custom folder", "custom")
        self.save_mode.currentIndexChanged.connect(self._save_mode_changed)
        cfg.addWidget(self._field("Save location", self.save_mode))
        self.custom_save = QLineEdit()
        self.custom_save.setPlaceholderText(r"D:\Saves\Game")
        custom_row = QWidget()
        cr = QHBoxLayout(custom_row)
        cr.setContentsMargins(0, 0, 0, 0)
        cr.setSpacing(8)
        cr.addWidget(self.custom_save, 1)
        custom_browse = QPushButton("Browse")
        custom_browse.clicked.connect(self._browse_custom_save)
        cr.addWidget(custom_browse)
        self.custom_save_wrap = self._field("Custom save path", custom_row)
        cfg.addWidget(self.custom_save_wrap)
        columns.addWidget(config_card, 1)
        page.addLayout(columns)

        # Overlay / compatibility.
        features_card, features = self._card()
        features.addWidget(self._section_title(
            "Overlay & compatibility",
            "GSE Experimental exposes the complete achievement overlay controls. ColdClient compatibility stays separate."
        ))
        self.gse_feature_pane = QWidget()
        fl = QVBoxLayout(self.gse_feature_pane)
        fl.setContentsMargins(0, 0, 0, 0)
        fl.setSpacing(10)

        # Clear master switch. This writes enable_experimental_overlay directly.
        self.overlay = ToggleSwitch()
        self.overlay.toggled.connect(self._overlay_master_changed)
        master = QFrame()
        master.setObjectName("InsetCard")
        ml = QVBoxLayout(master)
        ml.setContentsMargins(16, 12, 16, 12)
        ml.setSpacing(6)
        ml.addWidget(self._setting_row(
            "Enable GSE overlay",
            "Master switch for enable_experimental_overlay. Turning it on automatically selects Experimental GSE.",
            self.overlay,
        ))
        fl.addWidget(master)

        # Notification / visibility controls.
        notifications = QFrame()
        notifications.setObjectName("InsetCard")
        nl = QVBoxLayout(notifications)
        nl.setContentsMargins(16, 12, 16, 12)
        nl.setSpacing(10)
        self.overlay_achievement_notifications = ToggleSwitch(); self.overlay_achievement_notifications.setChecked(True)
        self.overlay_achievement_progress = ToggleSwitch()
        self.overlay_friend_notifications = ToggleSwitch(); self.overlay_friend_notifications.setChecked(True)
        self.overlay_icons = ToggleSwitch(); self.overlay_icons.setChecked(True)
        self.overlay_user_info = ToggleSwitch()
        self.overlay_warnings = ToggleSwitch(); self.overlay_warnings.setChecked(True)
        self.overlay_fps = ToggleSwitch()
        self.overlay_frametime = ToggleSwitch()
        self.overlay_show_playtime = ToggleSwitch()
        self.overlay_playtime = ToggleSwitch()
        nl.addWidget(self._setting_row("Achievement popup", "Show achievement unlock notifications.", self.overlay_achievement_notifications))
        nl.addWidget(self._setting_row("Achievement progress", "Show progress notifications for stat-linked achievements.", self.overlay_achievement_progress))
        nl.addWidget(self._setting_row("Friend notifications", "Show invitations and friend/chat notifications.", self.overlay_friend_notifications))
        nl.addWidget(self._setting_row("Achievement icons", "Upload achievement icons to the GPU for notifications and overlay lists.", self.overlay_icons))
        nl.addWidget(self._setting_row("Show user info", "Always show user identity in the overlay.", self.overlay_user_info))
        nl.addWidget(self._setting_row("Overlay warnings", "Keep GSE overlay warnings visible.", self.overlay_warnings))
        nl.addWidget(self._setting_row("FPS counter", "Always show FPS counter on screen.", self.overlay_fps))
        nl.addWidget(self._setting_row("Frametime", "Always show frametime metrics.", self.overlay_frametime))
        nl.addWidget(self._setting_row("Show playtime", "Show current recorded playtime inside the overlay.", self.overlay_show_playtime))
        nl.addWidget(self._setting_row("Record playtime", "Record GSE playtime to the save folder every minute.", self.overlay_playtime))
        fl.addWidget(notifications)

        # Appearance / renderer tuning.
        appearance = QFrame()
        appearance.setObjectName("InsetCard")
        al = QVBoxLayout(appearance)
        al.setContentsMargins(16, 12, 16, 12)
        al.setSpacing(9)
        self.overlay_position = QComboBox()
        for label, value in [
            ("Bottom right (Steam-like)", "bot_right"), ("Top right", "top_right"),
            ("Bottom left", "bot_left"), ("Top left", "top_left"),
            ("Bottom center", "bot_center"), ("Top center", "top_center"),
        ]:
            self.overlay_position.addItem(label, value)
        self.overlay_hotkey = QLineEdit("shift + tab")
        self.overlay_font_size = QDoubleSpinBox(); self.overlay_font_size.setRange(10.0, 64.0); self.overlay_font_size.setDecimals(1); self.overlay_font_size.setValue(20.0)
        self.overlay_icon_size = QDoubleSpinBox(); self.overlay_icon_size.setRange(16.0, 256.0); self.overlay_icon_size.setDecimals(1); self.overlay_icon_size.setValue(64.0)
        self.overlay_rounding = QDoubleSpinBox(); self.overlay_rounding.setRange(0.0, 32.0); self.overlay_rounding.setDecimals(1); self.overlay_rounding.setValue(10.0)
        self.overlay_animation = QDoubleSpinBox(); self.overlay_animation.setRange(0.0, 3.0); self.overlay_animation.setSingleStep(0.05); self.overlay_animation.setDecimals(2); self.overlay_animation.setValue(0.35)
        self.overlay_achievement_duration = QDoubleSpinBox(); self.overlay_achievement_duration.setRange(1.0, 30.0); self.overlay_achievement_duration.setSingleStep(0.5); self.overlay_achievement_duration.setDecimals(1); self.overlay_achievement_duration.setValue(7.0)
        self.overlay_hook_delay = QSpinBox(); self.overlay_hook_delay.setRange(0, 60); self.overlay_hook_delay.setValue(0)
        self.overlay_renderer_timeout = QSpinBox(); self.overlay_renderer_timeout.setRange(1, 120); self.overlay_renderer_timeout.setValue(15)
        line1 = QHBoxLayout()
        line1.addWidget(self._field("Achievement position", self.overlay_position), 2)
        line1.addWidget(self._field("Overlay hotkey", self.overlay_hotkey), 2)
        line1.addWidget(self._field("Font size", self.overlay_font_size), 1)
        line1.addWidget(self._field("Icon size", self.overlay_icon_size), 1)
        al.addLayout(line1)
        line2 = QHBoxLayout()
        line2.addWidget(self._field("Popup rounding", self.overlay_rounding), 1)
        line2.addWidget(self._field("Popup animation", self.overlay_animation, "seconds"), 1)
        line2.addWidget(self._field("Achievement duration", self.overlay_achievement_duration, "seconds"), 1)
        line2.addWidget(self._field("Hook delay", self.overlay_hook_delay, "seconds before renderer detection"), 1)
        line2.addWidget(self._field("Renderer timeout", self.overlay_renderer_timeout, "seconds"), 1)
        al.addLayout(line2)
        fl.addWidget(appearance)

        self.official_generator = ToggleSwitch()
        self.official_generator.setChecked(True)
        fl.addWidget(self._setting_row(
            "Official GSE generator",
            "Required for full GSE config (branches, depots, controllers, app/overlay INIs). Disable only for limited Web API fallback.",
            self.official_generator,
        ))

        self.coldclient_pane = QFrame()
        self.coldclient_pane.setObjectName("InsetCard")
        cl = QVBoxLayout(self.coldclient_pane)
        cl.setContentsMargins(16, 12, 16, 12)
        cl.setSpacing(10)
        self.coldclient_renderer = ToggleSwitch()
        self.coldclient_renderer.setChecked(True)
        self.coldclient_extra = ToggleSwitch()
        cl.addWidget(self._setting_row("GameOverlayRenderer", "Deploy official GameOverlayRenderer(64).dll compatibility stub.", self.coldclient_renderer))
        cl.addWidget(self._setting_row("Extra steamclient DLL", "Optional steamclient_experimental extra compatibility DLL.", self.coldclient_extra))
        fl.addWidget(self.coldclient_pane)

        # DInput8 overlay bridge row (for DX12 games that block keyboard hooks).
        self.dinput_bridge_pane = QFrame()
        self.dinput_bridge_pane.setObjectName("InsetCard")
        db = QHBoxLayout(self.dinput_bridge_pane)
        db.setContentsMargins(16, 12, 16, 12)
        self.overlay_dinput_bridge = ToggleSwitch()
        db.addWidget(self._setting_row(
            "DInput8 Overlay Bridge",
            "Copy dinput8.dll + dinput8.ini into the game folder. Required for overlay hotkey on DX12 games (e.g. RE Engine). Use only if Shift+Tab does not work.",
            self.overlay_dinput_bridge,
        ))
        fl.addWidget(self.dinput_bridge_pane)

        features.addWidget(self.gse_feature_pane)
        self.uc_feature_note = QLabel("UC Online uses the real Steam overlay path. Game-specific online behavior is provided by UC plugins rather than GSE's achievement overlay.")
        self.uc_feature_note.setObjectName("Hint")
        self.uc_feature_note.setWordWrap(True)
        features.addWidget(self.uc_feature_note)
        self.rune_feature_note = QLabel("RUNE AutoCracker automatically extracts interface version strings from the original DLL and populates the DLC list from the Steam Store API into steam_emu.ini / steak_emu.ini.")
        self.rune_feature_note.setObjectName("Hint")
        self.rune_feature_note.setWordWrap(True)
        features.addWidget(self.rune_feature_note)
        page.addWidget(features_card)

        # Tools & resources.
        tools_columns = QHBoxLayout()
        tools_columns.setSpacing(14)
        tools_card, tools = self._card()
        tools.addWidget(self._section_title("Tools", "Migration is never run automatically."))
        migrate = QPushButton("Migrate old Goldberg settings")
        migrate.clicked.connect(self._launch_migrate)
        tools.addWidget(migrate)
        self.reduced_motion = ToggleSwitch()
        self.reduced_motion.toggled.connect(self._apply_reduced_motion)
        tools.addWidget(self._setting_row("Reduced motion", "Disable toggle/drawer/progress interpolation animations.", self.reduced_motion))
        open_cfg = QPushButton("Open config.ini")
        open_cfg.clicked.connect(self._open_config)
        tools.addWidget(open_cfg)
        tools_columns.addWidget(tools_card, 1)

        resources_card, resources = self._card()
        resources.addWidget(self._section_title("Resources & updates", "Portable update overrides live beside the EXE. Large package caches are not kept in LocalAppData."))
        self.resource_gse = QLabel("GSE · checking local baseline")
        self.resource_uc = QLabel("UC Online · checking local baseline")
        self.resource_steamless = QLabel("Steamless · checking local baseline")
        self.resource_rune = QLabel("RUNE SteamStub · on-demand")
        self.resource_migrate = QLabel("migrate_gse · checking local baseline")
        for label in (self.resource_gse, self.resource_uc, self.resource_steamless, self.resource_rune, self.resource_migrate):
            label.setObjectName("Muted")
            resources.addWidget(label)
        resource_actions = QHBoxLayout()
        check = QPushButton("Check updates")
        check.clicked.connect(self._check_resource_updates)
        open_updates = QPushButton("Open updates folder")
        open_updates.clicked.connect(self._open_updates)
        clean = QPushButton("Clean update temp")
        clean.clicked.connect(self._clean_update_temp)
        resource_actions.addWidget(check)
        resource_actions.addWidget(open_updates)
        resource_actions.addWidget(clean)
        resources.addLayout(resource_actions)
        tools_columns.addWidget(resources_card, 1)
        page.addLayout(tools_columns)

        # Activity drawer.
        activity_card, activity = self._card()
        self.activity_header = QPushButton("Activity  ▾")
        self.activity_header.setObjectName("ActivityHeader")
        self.activity_header.clicked.connect(self._toggle_activity)
        activity.addWidget(self.activity_header)
        self.activity_body = AnimatedBody()
        abl = QVBoxLayout(self.activity_body)
        abl.setContentsMargins(0, 2, 0, 0)
        abl.setSpacing(8)
        self.progress = QProgressBar()
        self.progress.setRange(0, 100)
        self.progress.setValue(0)
        self.progress_label = QLabel("Ready")
        self.progress_label.setObjectName("Muted")
        self.log = QPlainTextEdit()
        self.log.setReadOnly(True)
        self.log.setMinimumHeight(175)
        abl.addWidget(self.progress)
        abl.addWidget(self.progress_label)
        abl.addWidget(self.log)
        activity.addWidget(self.activity_body)
        page.addWidget(activity_card)

        # Footer actions.
        actions = QHBoxLayout()
        actions.addStretch(1)
        self.restore_button = QPushButton("Restore original")
        self.restore_button.clicked.connect(self._restore)
        self.setup_button = QPushButton("Setup")
        self.setup_button.setObjectName("Primary")
        self.setup_button.clicked.connect(self._setup)
        actions.addWidget(self.restore_button)
        actions.addWidget(self.setup_button)
        page.addLayout(actions)
        page.addSpacing(8)

    # ---------- context / settings ----------
    def _all_toggles(self) -> list[ToggleSwitch]:
        return [
            self.remember_api, self.uc_auto_plugins, self.uc_eos, self.uc_photon,
            self.uc_playfab, self.uc_coherence, self.overlay,
            self.overlay_achievement_notifications, self.overlay_achievement_progress,
            self.overlay_friend_notifications, self.overlay_icons, self.overlay_user_info,
            self.overlay_warnings, self.overlay_fps, self.overlay_frametime,
            self.overlay_show_playtime, self.overlay_playtime, self.official_generator,
            self.coldclient_renderer, self.coldclient_extra, self.overlay_dinput_bridge,
            self.rune_unlock_all, self.rune_lobby, self.rune_overlays, self.rune_offline,
            self.reduced_motion,
        ]

    def _apply_reduced_motion(self, checked: bool) -> None:
        for toggle in self._all_toggles():
            toggle.set_reduced_motion(checked)
        self.activity_body.set_reduced_motion(checked)

    def _overlay_master_changed(self, checked: bool) -> None:
        controls = (
            self.overlay_achievement_notifications, self.overlay_achievement_progress,
            self.overlay_friend_notifications, self.overlay_icons, self.overlay_user_info,
            self.overlay_warnings, self.overlay_fps, self.overlay_frametime,
            self.overlay_show_playtime, self.overlay_position, self.overlay_hotkey,
            self.overlay_font_size, self.overlay_icon_size, self.overlay_rounding,
            self.overlay_animation, self.overlay_achievement_duration,
            self.overlay_hook_delay, self.overlay_renderer_timeout,
        )
        for control in controls:
            control.setEnabled(bool(checked))
        if checked and self._engine() == "gse":
            variant = self._current_segment(self.gse_variant_buttons, "regular")
            if variant == "regular":
                self.gse_variant_buttons["experimental"].setChecked(True)
        self._refresh_context()

    def _engine(self) -> str:
        if self.engine_rune.isChecked():
            return "rune"
        if self.engine_uc.isChecked():
            return "uc"
        return "gse"

    def _refresh_context(self) -> None:
        engine = self._engine()
        is_gse = engine == "gse"
        is_uc = engine == "uc"
        is_rune = engine == "rune"

        self.gse_pane.setVisible(is_gse)
        self.uc_pane.setVisible(is_uc)
        self.rune_pane.setVisible(is_rune)

        self.gse_feature_pane.setVisible(is_gse)
        self.uc_feature_note.setVisible(is_uc)
        self.rune_feature_note.setVisible(is_rune)

        self.account_name.setEnabled(is_gse)
        self.save_mode.setEnabled(is_gse)
        self.custom_save_wrap.setVisible(is_gse and self.save_mode.currentData() == "custom")

        if is_rune:
            self.engine_chip.setText("RUNE AutoCracker")
            self.engine_chip.setObjectName("ChipPurple")
        elif is_uc:
            self.engine_chip.setText("UC Online2")
            self.engine_chip.setObjectName("ChipGreen")
        else:
            self.engine_chip.setText("GSE")
            self.engine_chip.setObjectName("ChipBlue")
        self.engine_chip.style().unpolish(self.engine_chip)
        self.engine_chip.style().polish(self.engine_chip)

        variant = self._current_segment(self.gse_variant_buttons, "regular")
        # Full ColdClient options pane only for the loader-based variant.
        self.coldclient_pane.setVisible(is_gse and variant == "coldclient")
        # DInput8 bridge only for regular/experimental (not for either ColdClient variant, UC, or RUNE).
        self.dinput_bridge_pane.setVisible(is_gse and variant not in {"coldclient", "coldclient_simple"})
        if is_gse and self.overlay.isChecked() and variant == "regular":
            self.gse_variant_buttons["experimental"].setChecked(True)
            variant = "experimental"
        # UC runtime DRM has meaning only in UC mode. If context changed away, choose Auto.
        if is_gse and self.steamstub_mode.currentData() == "uc_runtime":
            self._set_combo_data(self.steamstub_mode, "auto")
        self._save_mode_changed()

    def _save_mode_changed(self) -> None:
        self.custom_save_wrap.setVisible(
            self._engine() == "gse" and self.save_mode.currentData() == "custom"
        )

    def _set_combo_data(self, combo: QComboBox, value: str) -> None:
        idx = combo.findData(value)
        if idx >= 0:
            combo.setCurrentIndex(idx)

    def _switch_workspace(self, index: int) -> None:
        index = 1 if int(index) == 1 else 0
        self.workspace_stack.setCurrentIndex(index)
        self.setup_tab_button.setChecked(index == 0)
        self.save_tab_button.setChecked(index == 1)
        self.engine_chip.setText("Save Manager" if index == 1 else self._engine().upper())
        if index == 1:
            self.save_manager_tab.refresh_drive_status()

    def _load_settings(self) -> None:
        cfg = load_config(self.config_path)
        self.appid.setText(cfg.last_appid)
        self.game_folder.setText(cfg.last_game_folder)
        self.api_key.setText(cfg.steam_web_api_key)
        self.remember_api.setChecked(cfg.remember_web_api_key)
        self.account_name.setText(cfg.account_name or "0xoLemon")
        self._set_combo_data(self.save_mode, cfg.save_mode)
        self.custom_save.setText(cfg.custom_save_path)
        if cfg.engine == "rune":
            self.engine_rune.setChecked(True)
        elif cfg.engine == "uc":
            self.engine_uc.setChecked(True)
        else:
            self.engine_gse.setChecked(True)
        variant = cfg.gse_variant or cfg.gse_build or "regular"
        if variant in self.gse_variant_buttons:
            self.gse_variant_buttons[variant].setChecked(True)
        if cfg.network_mode in self.network_buttons:
            self.network_buttons[cfg.network_mode].setChecked(True)
        rune_prof = cfg.rune_profile or "regular"
        if rune_prof in self.rune_profile_buttons:
            self.rune_profile_buttons[rune_prof].setChecked(True)
        self.rune_username.setText(cfg.rune_username or "RUNE")
        self.rune_language.setText(cfg.rune_language or "english")
        self.rune_unlock_all.setChecked(cfg.rune_unlock_all_dlcs)
        self.rune_lobby.setChecked(cfg.rune_lobby)
        self.rune_overlays.setChecked(cfg.rune_overlays)
        self.rune_offline.setChecked(cfg.rune_offline)
        self._set_combo_data(self.steamstub_mode, cfg.steamstub_mode or "auto")
        self.uc_spoof.setText(str(cfg.uc_spoof_appid or 480))
        plugins = {x.strip().lower() for x in (cfg.uc_plugins or "").split(",") if x.strip()}
        self.uc_auto_plugins.setChecked("auto" in plugins or not plugins)
        self.uc_eos.setChecked("eos" in plugins)
        self.uc_photon.setChecked("photon" in plugins)
        self.uc_playfab.setChecked("playfab" in plugins)
        self.uc_coherence.setChecked("coherence" in plugins)
        self.overlay.setChecked(cfg.overlay)
        self.overlay_achievement_notifications.setChecked(cfg.overlay_achievement_notifications)
        self.overlay_friend_notifications.setChecked(cfg.overlay_friend_notifications)
        self.overlay_achievement_progress.setChecked(cfg.overlay_achievement_progress)
        self.overlay_icons.setChecked(cfg.overlay_icons)
        self.overlay_user_info.setChecked(cfg.overlay_user_info)
        self.overlay_warnings.setChecked(cfg.overlay_warnings)
        self.overlay_fps.setChecked(cfg.overlay_fps)
        self.overlay_frametime.setChecked(cfg.overlay_frametime)
        self.overlay_show_playtime.setChecked(cfg.overlay_show_playtime)
        self.overlay_playtime.setChecked(cfg.overlay_playtime)
        self._set_combo_data(self.overlay_position, cfg.overlay_position or "bot_right")
        self.overlay_hotkey.setText(cfg.overlay_hotkey or "shift + tab")
        self.overlay_font_size.setValue(float(cfg.overlay_font_size))
        self.overlay_icon_size.setValue(float(cfg.overlay_icon_size))
        self.overlay_rounding.setValue(float(cfg.overlay_rounding))
        self.overlay_animation.setValue(float(cfg.overlay_animation))
        self.overlay_achievement_duration.setValue(float(cfg.overlay_achievement_duration))
        self.overlay_hook_delay.setValue(int(cfg.overlay_hook_delay))
        self.overlay_renderer_timeout.setValue(int(cfg.overlay_renderer_timeout))
        self.official_generator.setChecked(cfg.official_generator)
        self.coldclient_renderer.setChecked(cfg.coldclient_renderer)
        self.coldclient_extra.setChecked(cfg.coldclient_extra)
        self.overlay_dinput_bridge.setChecked(cfg.overlay_dinput_bridge)
        self.reduced_motion.setChecked(cfg.reduced_motion)
        self._apply_reduced_motion(cfg.reduced_motion)
        if getattr(cfg, "save_manager_root", ""):
            self.save_manager_tab.root_edit.setText(cfg.save_manager_root)
        self._switch_workspace(1 if getattr(cfg, "last_top_tab", "setup") == "save_manager" else 0)

    def _selected_uc_plugins(self) -> tuple[str, ...]:
        out: list[str] = []
        if self.uc_auto_plugins.isChecked():
            out.append("auto")
        if self.uc_eos.isChecked():
            out.append("eos")
        if self.uc_photon.isChecked():
            out.append("photon")
        if self.uc_playfab.isChecked():
            out.append("playfab")
        if self.uc_coherence.isChecked():
            out.append("coherence")
        return tuple(out)

    def _collect_config(self) -> ToolConfig:
        variant = self._current_segment(self.gse_variant_buttons, "regular")
        network = self._current_segment(self.network_buttons, "singleplayer")
        try:
            spoof = int(self.uc_spoof.text().strip() or "480")
        except ValueError:
            spoof = 480
        return ToolConfig(
            last_appid=self.appid.text().strip(),
            last_game_folder=self.game_folder.text().strip(),
            account_name=self.account_name.text().strip() or "0xoLemon",
            save_mode=str(self.save_mode.currentData() or "gse"),
            custom_save_path=self.custom_save.text().strip(),
            gse_build=variant,
            overlay=self.overlay.isChecked(),
            official_generator=self.official_generator.isChecked(),
            deployment_mode="replace",
            steamstub=self.steamstub_mode.currentData() != "disabled",
            engine=self._engine(),
            gse_variant=variant,
            network_mode=network,
            steamstub_mode=str(self.steamstub_mode.currentData() or "auto"),
            uc_spoof_appid=spoof,
            uc_plugins=",".join(self._selected_uc_plugins()),
            coldclient_renderer=self.coldclient_renderer.isChecked(),
            coldclient_extra=self.coldclient_extra.isChecked(),
            reduced_motion=self.reduced_motion.isChecked(),
            overlay_fps=self.overlay_fps.isChecked(),
            overlay_frametime=self.overlay_frametime.isChecked(),
            overlay_playtime=self.overlay_playtime.isChecked(),
            overlay_achievement_notifications=self.overlay_achievement_notifications.isChecked(),
            overlay_friend_notifications=self.overlay_friend_notifications.isChecked(),
            overlay_achievement_progress=self.overlay_achievement_progress.isChecked(),
            overlay_icons=self.overlay_icons.isChecked(),
            overlay_user_info=self.overlay_user_info.isChecked(),
            overlay_show_playtime=self.overlay_show_playtime.isChecked(),
            overlay_position=str(self.overlay_position.currentData() or "bot_right"),
            overlay_hotkey=self.overlay_hotkey.text().strip() or "shift + tab",
            overlay_font_size=float(self.overlay_font_size.value()),
            overlay_icon_size=float(self.overlay_icon_size.value()),
            overlay_rounding=float(self.overlay_rounding.value()),
            overlay_animation=float(self.overlay_animation.value()),
            overlay_achievement_duration=float(self.overlay_achievement_duration.value()),
            overlay_hook_delay=int(self.overlay_hook_delay.value()),
            overlay_renderer_timeout=int(self.overlay_renderer_timeout.value()),
            overlay_warnings=self.overlay_warnings.isChecked(),
            overlay_dinput_bridge=self.overlay_dinput_bridge.isChecked(),
            rune_profile=self._current_segment(self.rune_profile_buttons, "regular"),
            rune_username=self.rune_username.text().strip() or "RUNE",
            rune_language=self.rune_language.text().strip() or "english",
            rune_unlock_all_dlcs=self.rune_unlock_all.isChecked(),
            rune_lobby=self.rune_lobby.isChecked(),
            rune_overlays=self.rune_overlays.isChecked(),
            rune_offline=self.rune_offline.isChecked(),
            remember_web_api_key=self.remember_api.isChecked(),
            steam_web_api_key=self.api_key.text().strip() if self.remember_api.isChecked() else "",
            last_top_tab="save_manager" if self.workspace_stack.currentIndex() == 1 else "setup",
            save_manager_root=self.save_manager_tab.root_edit.text().strip(),
        )

    def _save_settings(self, silent: bool = False) -> None:
        try:
            path = save_config(self._collect_config(), self.config_path)
            if not silent:
                self._append_log(f"Settings saved: {path}")
        except Exception as exc:
            if not silent:
                QMessageBox.warning(self, "Settings", f"Could not save config.ini:\n{exc}")

    # ---------- actions ----------
    def _browse_game(self) -> None:
        folder = QFileDialog.getExistingDirectory(self, "Select game folder", self.game_folder.text() or str(Path.home()))
        if folder:
            self.game_folder.setText(folder)

    def _browse_custom_save(self) -> None:
        folder = QFileDialog.getExistingDirectory(self, "Select save folder", self.custom_save.text() or str(Path.home()))
        if folder:
            self.custom_save.setText(folder)

    def _open_config(self) -> None:
        self._save_settings(silent=True)
        QDesktopServices.openUrl(QUrl.fromLocalFile(str(self.config_path)))

    def _open_updates(self) -> None:
        root = ResourceManager().updates_root
        root.mkdir(parents=True, exist_ok=True)
        QDesktopServices.openUrl(QUrl.fromLocalFile(str(root)))

    def _clean_update_temp(self) -> None:
        ResourceManager().clear_temp()
        self._append_log("Portable update temp cleaned.")

    def _launch_migrate(self) -> None:
        answer = QMessageBox.question(
            self,
            "Migrate old Goldberg settings",
            "Launch the official migrate_gse utility?\n\nIt is an explicit tool and will not run as part of Setup.",
        )
        if answer != QMessageBox.Yes:
            return
        try:
            MigrateGSEManager(log=self._append_log).launch()
            self._append_log("migrate_gse launched.")
        except Exception as exc:
            QMessageBox.critical(self, "migrate_gse", str(exc))

    def _refresh_local_resource_labels(self) -> None:
        rm = ResourceManager()
        mapping = [
            ("gse", self.resource_gse, "GSE"),
            ("uc_online", self.resource_uc, "UC Online"),
            ("steamless", self.resource_steamless, "Steamless"),
            ("rune_steamstub", self.resource_rune, "RUNE SteamStub"),
            ("migrate_gse", self.resource_migrate, "migrate_gse"),
        ]
        for component, label, title in mapping:
            update = rm.update_root(component)
            embedded = rm.embedded_root(component)
            if update.is_dir() and any(update.iterdir()):
                state = "portable update"
            elif embedded.is_dir() and any(embedded.iterdir()):
                state = "embedded baseline"
            else:
                state = "download on demand"
            label.setText(f"{title} · {state}")

    def _check_resource_updates(self) -> None:
        if self._resource_thread and self._resource_thread.isRunning():
            return
        self._append_log("Checking component releases...")
        thread = QThread(self)
        worker = ResourceStatusWorker()
        worker.moveToThread(thread)
        thread.started.connect(worker.run)
        worker.finished.connect(self._resource_check_finished)
        worker.failed.connect(lambda msg: self._append_log(f"Resource check failed: {msg}"))
        worker.finished.connect(thread.quit)
        worker.failed.connect(thread.quit)
        thread.finished.connect(worker.deleteLater)
        thread.finished.connect(thread.deleteLater)
        self._resource_thread = thread
        self._resource_worker = worker
        thread.start()

    def _resource_check_finished(self, data: dict) -> None:
        self._refresh_local_resource_labels()
        self.resource_gse.setText(self.resource_gse.text() + f" · latest {data.get('gse', '?')}")
        self.resource_uc.setText(self.resource_uc.text() + f" · latest {data.get('uc', '?')}")
        self.resource_rune.setText(self.resource_rune.text() + f" · latest {data.get('rune', '?')}")
        self.resource_steamless.setText(self.resource_steamless.text() + f" · latest {data.get('steamless', '?')}")
        self._append_log("Component release check complete. Updates are installed on demand during Setup.")

    def _toggle_activity(self) -> None:
        target = not self.activity_body._expanded
        self.activity_body.set_expanded(target)
        self.activity_header.setText("Activity  ▴" if target else "Activity  ▾")

    def _ensure_activity_open(self) -> None:
        if not self.activity_body._expanded:
            self._toggle_activity()

    def _append_log(self, message: str) -> None:
        stamp = datetime.now().strftime("%H:%M:%S")
        self.log.appendPlainText(f"[{stamp}] {message}")

    def _set_progress(self, value: int, text: str) -> None:
        self.progress_label.setText(text)
        value = max(0, min(100, int(value)))
        if self.reduced_motion.isChecked():
            self.progress.setValue(value)
            return
        if self._progress_animation:
            self._progress_animation.stop()
        self._progress_animation = QPropertyAnimation(self.progress, b"value", self)
        self._progress_animation.setDuration(180)
        self._progress_animation.setStartValue(self.progress.value())
        self._progress_animation.setEndValue(value)
        self._progress_animation.setEasingCurve(QEasingCurve.OutCubic)
        self._progress_animation.start()

    def _set_busy(self, busy: bool) -> None:
        self.setup_button.setEnabled(not busy)
        self.restore_button.setEnabled(not busy)

    def _setup(self) -> None:
        self._save_settings(silent=True)
        engine = self._engine()
        try:
            appid = int(self.appid.text().strip())
        except ValueError:
            QMessageBox.warning(self, "Input", "AppID must be a number.")
            return
        game = Path(self.game_folder.text().strip())
        api_key = self.api_key.text().strip()
        try:
            validate_inputs(self.appid.text(), self.game_folder.text(), api_key, engine=engine)
        except Exception as exc:
            QMessageBox.warning(self, "Input", str(exc))
            return
        if engine == "uc":
            try:
                spoof = int(self.uc_spoof.text().strip() or "480")
            except ValueError:
                QMessageBox.warning(self, "Input", "UC spoof AppID must be a number.")
                return
            if spoof <= 0:
                QMessageBox.warning(self, "Input", "UC spoof AppID must be positive.")
                return
        else:
            spoof = 480

        variant = self._current_segment(self.gse_variant_buttons, "regular")
        network = self._current_segment(self.network_buttons, "singleplayer")
        inputs = Inputs(
            appid=appid,
            game_folder=game,
            api_key=api_key,
            use_official_generator=self.official_generator.isChecked(),
            account_name=self.account_name.text().strip() or "0xoLemon",
            save_mode=str(self.save_mode.currentData() or "gse"),
            custom_save_path=self.custom_save.text().strip(),
            enable_overlay=self.overlay.isChecked(),
            engine=engine,
            gse_variant=variant,
            network_mode=network,
            steamstub_mode=str(self.steamstub_mode.currentData() or "auto"),
            uc_spoof_appid=spoof,
            uc_plugins=self._selected_uc_plugins(),
            coldclient_renderer=self.coldclient_renderer.isChecked(),
            coldclient_extra=self.coldclient_extra.isChecked(),
            overlay_fps=self.overlay_fps.isChecked(),
            overlay_frametime=self.overlay_frametime.isChecked(),
            overlay_playtime=self.overlay_playtime.isChecked(),
            overlay_achievement_notifications=self.overlay_achievement_notifications.isChecked(),
            overlay_friend_notifications=self.overlay_friend_notifications.isChecked(),
            overlay_achievement_progress=self.overlay_achievement_progress.isChecked(),
            overlay_icons=self.overlay_icons.isChecked(),
            overlay_user_info=self.overlay_user_info.isChecked(),
            overlay_show_playtime=self.overlay_show_playtime.isChecked(),
            overlay_position=str(self.overlay_position.currentData() or "bot_right"),
            overlay_hotkey=self.overlay_hotkey.text().strip() or "shift + tab",
            overlay_font_size=float(self.overlay_font_size.value()),
            overlay_icon_size=float(self.overlay_icon_size.value()),
            overlay_rounding=float(self.overlay_rounding.value()),
            overlay_animation=float(self.overlay_animation.value()),
            overlay_achievement_duration=float(self.overlay_achievement_duration.value()),
            overlay_hook_delay=int(self.overlay_hook_delay.value()),
            overlay_renderer_timeout=int(self.overlay_renderer_timeout.value()),
            overlay_warnings=self.overlay_warnings.isChecked(),
            overlay_dinput_bridge=self.overlay_dinput_bridge.isChecked(),
            rune_profile=self._current_segment(self.rune_profile_buttons, "regular"),
            rune_username=self.rune_username.text().strip() or "RUNE",
            rune_language=self.rune_language.text().strip() or "english",
            rune_unlock_all_dlcs=self.rune_unlock_all.isChecked(),
            rune_lobby=self.rune_lobby.isChecked(),
            rune_overlays=self.rune_overlays.isChecked(),
            rune_offline=self.rune_offline.isChecked(),
        )
        if engine == "gse" and inputs.enable_overlay and variant == "regular":
            self.gse_variant_buttons["experimental"].setChecked(True)
            inputs = Inputs(**{**inputs.__dict__, "gse_variant": "experimental", "experimental": True})

        self._ensure_activity_open()
        self.log.clear()
        self._append_log(f"Starting {engine.upper()} setup...")
        self._set_progress(2, "Preparing")
        self._set_busy(True)
        self._run_worker(SetupWorker(inputs), self._setup_finished)

    def _restore(self) -> None:
        game = Path(self.game_folder.text().strip())
        if not game.is_dir():
            QMessageBox.warning(self, "Restore", "Select a valid game folder first.")
            return
        answer = QMessageBox.question(
            self,
            "Restore original",
            "Restore the game-local transaction snapshot and remove files created by the setup tool?",
        )
        if answer != QMessageBox.Yes:
            return
        self._ensure_activity_open()
        self._append_log("Restoring original files...")
        self._set_busy(True)
        self._run_worker(RestoreWorker(game), self._restore_finished)

    def _run_worker(self, worker: QObject, on_finished) -> None:
        thread = QThread(self)
        worker.moveToThread(thread)
        thread.started.connect(worker.run)
        worker.log.connect(self._append_log)
        worker.progress.connect(self._set_progress)
        worker.finished.connect(on_finished)
        worker.failed.connect(self._worker_failed)
        worker.finished.connect(thread.quit)
        worker.failed.connect(thread.quit)
        thread.finished.connect(worker.deleteLater)
        thread.finished.connect(thread.deleteLater)
        self._thread = thread
        self._worker = worker
        thread.start()

    def _setup_finished(self, result) -> None:
        self._set_busy(False)
        self._set_progress(100, "Ready")
        message = getattr(result, "message", None) or "Setup completed."
        self._append_log(message)
        QMessageBox.information(self, "Setup", message)
        self._refresh_local_resource_labels()

    def _restore_finished(self, result) -> None:
        self._set_busy(False)
        self._set_progress(100, "Restored")
        message = getattr(result, "message", None) or "Restore completed."
        self._append_log(message)
        QMessageBox.information(self, "Restore", message)

    def _worker_failed(self, message: str) -> None:
        self._set_busy(False)
        self._set_progress(0, "Failed")
        self._append_log(f"ERROR: {message}")
        QMessageBox.critical(self, "Operation failed", message)


def run_app() -> int:
    app = QApplication.instance() or QApplication(sys.argv)
    app.setApplicationName("GSE / UC Setup")
    app.setOrganizationName("GSEAutoSetup")
    app.setStyle("Fusion")
    window = MainWindow()
    window.show()
    return app.exec()
