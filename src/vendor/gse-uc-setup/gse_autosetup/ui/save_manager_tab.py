from __future__ import annotations

import os
from pathlib import Path
from typing import Callable

from PySide6.QtCore import QObject, QThread, Qt, Signal, Slot, QUrl
from PySide6.QtGui import QDesktopServices, QPixmap
from PySide6.QtWidgets import (
    QComboBox,
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QInputDialog,
    QLabel,
    QLineEdit,
    QMessageBox,
    QProgressBar,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from ..save_manager.models import SaveEntry
from ..save_manager.scanner import default_gse_saves_root
from ..save_manager.service import SaveManagerService


class SaveTaskWorker(QObject):
    finished = Signal(object)
    failed = Signal(str)
    progress = Signal(int, str)

    def __init__(self, task: Callable[[Callable[[int, str], None]], object]):
        super().__init__()
        self.task = task

    @Slot()
    def run(self):
        try:
            self.finished.emit(self.task(lambda p, m: self.progress.emit(int(p), str(m))))
        except Exception as exc:
            self.failed.emit(str(exc))


class SaveManagerTab(QWidget):
    """Second top-level workspace for local GSE saves and Drive backups."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.service = SaveManagerService()
        self._thread: QThread | None = None
        self._entries: list[SaveEntry] = []
        self._build_ui()
        self.refresh_drive_status()

    def _card(self, strong: bool = False):
        card = QFrame()
        card.setObjectName("GlassCardStrong" if strong else "GlassCard")
        layout = QVBoxLayout(card)
        layout.setContentsMargins(22, 20, 22, 20)
        layout.setSpacing(13)
        return card, layout

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        host = QWidget()
        outer = QHBoxLayout(host)
        outer.setContentsMargins(30, 8, 30, 24)
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
        scroll.setWidget(host)
        root.addWidget(scroll)

        title_card, title = self._card(strong=True)
        h = QHBoxLayout()
        text = QVBoxLayout()
        t = QLabel("Savegame Manager")
        t.setObjectName("PageTitle")
        s = QLabel("Manage complete GSE AppID save folders, local snapshots and Google Drive backups.")
        s.setObjectName("Muted")
        text.addWidget(t)
        text.addWidget(s)
        h.addLayout(text, 1)
        self.refresh_button = QPushButton("Refresh saves")
        self.refresh_button.clicked.connect(self.refresh_saves)
        h.addWidget(self.refresh_button)
        title.addLayout(h)

        source_row = QHBoxLayout()
        self.root_edit = QLineEdit(str(default_gse_saves_root()))
        self.root_edit.setPlaceholderText(r"%APPDATA%\GSE Saves")
        browse = QPushButton("Browse source")
        browse.clicked.connect(self._browse_root)
        default = QPushButton("Use GSE default")
        default.clicked.connect(lambda: self.root_edit.setText(str(default_gse_saves_root())))
        source_row.addWidget(self.root_edit, 1)
        source_row.addWidget(default)
        source_row.addWidget(browse)
        title.addLayout(source_row)

        tools = QHBoxLayout()
        self.search = QLineEdit()
        self.search.setPlaceholderText("Search game name or AppID...")
        self.search.textChanged.connect(self._render_entries)
        self.sort = QComboBox()
        self.sort.addItem("Recently modified", "modified")
        self.sort.addItem("Game name", "name")
        self.sort.addItem("AppID", "appid")
        self.sort.addItem("Save size", "size")
        self.sort.currentIndexChanged.connect(self._render_entries)
        tools.addWidget(self.search, 1)
        tools.addWidget(self.sort)
        title.addLayout(tools)
        page.addWidget(title_card)

        cloud_card, cloud = self._card()
        cloud_title = QLabel("Google Drive")
        cloud_title.setObjectName("SectionTitle")
        cloud.addWidget(cloud_title)
        self.drive_status = QLabel("Not connected")
        self.drive_status.setObjectName("Muted")
        cloud.addWidget(self.drive_status)
        drive_buttons = QHBoxLayout()
        self.connect_drive = QPushButton("Connect Google Drive")
        self.connect_drive.clicked.connect(self._connect_drive)
        self.backup_all_drive = QPushButton("Backup all to Drive")
        self.backup_all_drive.clicked.connect(self._backup_all_drive)
        self.browse_cloud = QPushButton("Browse cloud backups")
        self.browse_cloud.clicked.connect(self._browse_cloud)
        self.disconnect_drive = QPushButton("Disconnect")
        self.disconnect_drive.clicked.connect(self._disconnect_drive)
        drive_buttons.addWidget(self.connect_drive)
        drive_buttons.addWidget(self.backup_all_drive)
        drive_buttons.addWidget(self.browse_cloud)
        drive_buttons.addWidget(self.disconnect_drive)
        drive_buttons.addStretch(1)
        cloud.addLayout(drive_buttons)
        page.addWidget(cloud_card)

        self.progress_label = QLabel("Ready")
        self.progress_label.setObjectName("Muted")
        self.progress = QProgressBar()
        self.progress.setRange(0, 100)
        self.progress.setValue(0)
        page.addWidget(self.progress_label)
        page.addWidget(self.progress)

        self.entries_host = QWidget()
        self.entries_layout = QVBoxLayout(self.entries_host)
        self.entries_layout.setContentsMargins(0, 0, 0, 0)
        self.entries_layout.setSpacing(12)
        page.addWidget(self.entries_host)
        page.addStretch(1)

    def showEvent(self, event):
        super().showEvent(event)
        if not self._entries:
            self.refresh_saves()

    def _browse_root(self) -> None:
        folder = QFileDialog.getExistingDirectory(self, "Select GSE saves root", self.root_edit.text() or str(Path.home()))
        if folder:
            self.root_edit.setText(folder)
            self.refresh_saves()

    @staticmethod
    def _format_size(value: int) -> str:
        size = float(max(0, int(value)))
        for unit in ("B", "KB", "MB", "GB", "TB"):
            if size < 1024 or unit == "TB":
                return f"{size:.1f} {unit}" if unit != "B" else f"{int(size)} B"
            size /= 1024
        return f"{size:.1f} TB"

    def _set_busy(self, busy: bool, text: str = "Working...") -> None:
        self.refresh_button.setEnabled(not busy)
        self.connect_drive.setEnabled(not busy)
        self.backup_all_drive.setEnabled(not busy)
        if busy:
            self.progress_label.setText(text)

    def _run_task(self, task, on_finished, label: str) -> None:
        if self._thread and self._thread.isRunning():
            return
        self._set_busy(True, label)
        self.progress.setValue(0)
        thread = QThread(self)
        worker = SaveTaskWorker(task)
        worker.moveToThread(thread)
        thread.started.connect(worker.run)
        worker.progress.connect(self._task_progress)
        worker.finished.connect(on_finished)
        worker.failed.connect(self._task_failed)
        worker.finished.connect(thread.quit)
        worker.failed.connect(thread.quit)
        thread.finished.connect(worker.deleteLater)
        thread.finished.connect(thread.deleteLater)
        self._thread = thread
        self._worker = worker
        thread.start()

    def _task_progress(self, value: int, text: str) -> None:
        self.progress.setValue(max(0, min(100, int(value))))
        self.progress_label.setText(text)

    def _task_failed(self, message: str) -> None:
        self._set_busy(False)
        self.progress.setValue(0)
        self.progress_label.setText("Failed")
        QMessageBox.critical(self, "Savegame Manager", message)

    def refresh_saves(self) -> None:
        root = Path(self.root_edit.text().strip() or str(default_gse_saves_root()))

        def task(progress):
            progress(10, "Scanning GSE Saves...")
            entries = self.service.scan(root, resolve_metadata=True)
            progress(100, f"Found {len(entries)} save folders")
            return entries

        self._run_task(task, self._scan_finished, "Scanning saves...")

    def _scan_finished(self, result) -> None:
        self._set_busy(False)
        self._entries = list(result or [])
        self.progress.setValue(100)
        self.progress_label.setText(f"{len(self._entries)} save folders")
        self._render_entries()

    def _filtered_entries(self) -> list[SaveEntry]:
        query = self.search.text().strip().casefold()
        entries = [e for e in self._entries if not query or query in str(e.appid) or query in (e.game_name or "").casefold()]
        key = str(self.sort.currentData() or "modified")
        if key == "name":
            entries.sort(key=lambda e: (e.game_name or "").casefold())
        elif key == "appid":
            entries.sort(key=lambda e: e.appid)
        elif key == "size":
            entries.sort(key=lambda e: e.size_bytes, reverse=True)
        else:
            entries.sort(key=lambda e: e.modified_at, reverse=True)
        return entries

    def _clear_cards(self) -> None:
        while self.entries_layout.count():
            item = self.entries_layout.takeAt(0)
            widget = item.widget()
            if widget is not None:
                widget.deleteLater()

    def _render_entries(self, *_args) -> None:
        self._clear_cards()
        entries = self._filtered_entries()
        if not entries:
            empty = QLabel("No numeric AppID save folders were found in this source.")
            empty.setObjectName("Muted")
            self.entries_layout.addWidget(empty)
            return
        for entry in entries:
            self.entries_layout.addWidget(self._entry_card(entry))
        self.entries_layout.addStretch(1)

    def _entry_card(self, entry: SaveEntry) -> QWidget:
        card, body = self._card()
        row = QHBoxLayout()
        cover = QLabel()
        cover.setFixedSize(184, 69)
        cover.setAlignment(Qt.AlignCenter)
        cover.setObjectName("InsetCard")
        if entry.cover_path and Path(entry.cover_path).is_file():
            pix = QPixmap(str(entry.cover_path))
            if not pix.isNull():
                cover.setPixmap(pix.scaled(184, 69, Qt.KeepAspectRatioByExpanding, Qt.SmoothTransformation))
        else:
            cover.setText("STEAM")
        row.addWidget(cover)

        info = QVBoxLayout()
        name = QLabel(entry.game_name or f"Steam App {entry.appid}")
        name.setObjectName("SectionTitle")
        meta = QLabel(
            f"AppID {entry.appid} · {self._format_size(entry.size_bytes)} · "
            f"Modified {entry.modified_at.strftime('%Y-%m-%d %H:%M')} · "
            f"{entry.local_backup_count} local backup(s)"
        )
        meta.setObjectName("Muted")
        path = QLabel(str(entry.save_folder))
        path.setObjectName("Muted")
        path.setTextInteractionFlags(Qt.TextSelectableByMouse)
        info.addWidget(name)
        info.addWidget(meta)
        info.addWidget(path)
        row.addLayout(info, 1)

        actions = QVBoxLayout()
        open_btn = QPushButton("Open folder")
        open_btn.clicked.connect(lambda _=False, e=entry: QDesktopServices.openUrl(QUrl.fromLocalFile(str(e.save_folder))))
        backup_btn = QPushButton("Backup")
        backup_btn.clicked.connect(lambda _=False, e=entry: self._backup_entry(e))
        restore_btn = QPushButton("Restore")
        restore_btn.clicked.connect(lambda _=False, e=entry: self._restore_entry(e))
        drive_btn = QPushButton("Backup to Drive")
        drive_btn.clicked.connect(lambda _=False, e=entry: self._backup_entry_drive(e))
        cloud_history = QPushButton("Restore from Drive")
        cloud_history.clicked.connect(lambda _=False, e=entry: self._restore_entry_drive(e))
        actions.addWidget(open_btn)
        actions.addWidget(backup_btn)
        actions.addWidget(restore_btn)
        actions.addWidget(drive_btn)
        actions.addWidget(cloud_history)
        row.addLayout(actions)
        body.addLayout(row)
        return card

    def _backup_entry(self, entry: SaveEntry) -> None:
        def task(progress):
            progress(20, f"Backing up {entry.game_name or entry.appid}...")
            archive = self.service.backup(entry)
            progress(100, "Backup completed")
            return archive
        self._run_task(task, lambda archive: self._backup_finished(entry, archive), "Creating local backup...")

    def _backup_finished(self, entry: SaveEntry, archive) -> None:
        self._set_busy(False)
        self.progress.setValue(100)
        self.progress_label.setText("Backup completed")
        QMessageBox.information(self, "Save backup", f"Backup created:\n{archive}")
        self.refresh_saves()

    def _restore_entry(self, entry: SaveEntry) -> None:
        backups = self.service.backups.list_backups(entry.appid)
        if not backups:
            QMessageBox.information(self, "Restore", "No local backups exist for this AppID yet.")
            return
        labels = [p.name for p in backups]
        picked, ok = QInputDialog.getItem(self, "Restore save", "Choose backup:", labels, 0, False)
        if not ok or not picked:
            return
        archive = backups[labels.index(picked)]
        answer = QMessageBox.question(
            self,
            "Restore save",
            "The current AppID folder will be safety-backed-up, then replaced with this snapshot. Continue?",
        )
        if answer != QMessageBox.Yes:
            return

        def task(progress):
            progress(15, "Validating backup...")
            result = self.service.restore(archive, entry.source_root, appid=entry.appid)
            progress(100, "Restore completed")
            return result
        self._run_task(task, lambda result: self._restore_finished(entry, result), "Restoring save...")

    def _restore_finished(self, entry: SaveEntry, result) -> None:
        self._set_busy(False)
        self.progress.setValue(100)
        safety = getattr(result, "safety_backup", None)
        message = f"Restored AppID {entry.appid}."
        if safety:
            message += f"\nSafety backup: {safety}"
        QMessageBox.information(self, "Restore", message)
        self.refresh_saves()

    def refresh_drive_status(self) -> None:
        status = self.service.drive_status()
        connected = bool(status.get("connected"))
        user = status.get("user") or {}
        email = user.get("emailAddress") or user.get("displayName") or "Connected account"
        self.drive_status.setText(f"Connected · {email}" if connected else "Not connected")
        self.connect_drive.setVisible(not connected)
        self.disconnect_drive.setVisible(connected)
        self.backup_all_drive.setEnabled(connected)
        self.browse_cloud.setEnabled(connected)

    def _connect_drive(self) -> None:
        def task(progress):
            progress(10, "Opening Google sign-in in your browser...")
            info = self.service.connect_drive()
            progress(100, "Google Drive connected")
            return info
        self._run_task(task, self._drive_connected, "Connecting Google Drive...")

    def _drive_connected(self, _result) -> None:
        self._set_busy(False)
        self.progress.setValue(100)
        self.refresh_drive_status()
        QMessageBox.information(self, "Google Drive", "Google Drive connected.")

    def _disconnect_drive(self) -> None:
        self.service.disconnect_drive()
        self.refresh_drive_status()

    def _backup_entry_drive(self, entry: SaveEntry) -> None:
        if not self.service.auth.is_connected():
            QMessageBox.information(self, "Google Drive", "Connect Google Drive first.")
            return

        def task(progress):
            return self.service.backup_to_drive(entry, progress=lambda p: progress(p, f"Uploading {entry.game_name or entry.appid}... {p}%"))
        self._run_task(task, lambda result: self._drive_backup_finished(entry, result), "Preparing Drive backup...")

    def _drive_backup_finished(self, entry: SaveEntry, result) -> None:
        self._set_busy(False)
        self.progress.setValue(100)
        archive, uploaded = result
        QMessageBox.information(
            self,
            "Google Drive",
            f"Uploaded {entry.game_name or entry.appid}.\nLocal snapshot: {archive}\nDrive file: {uploaded.get('name', '')}",
        )
        self.refresh_saves()

    def _restore_entry_drive(self, entry: SaveEntry) -> None:
        if not self.service.auth.is_connected():
            QMessageBox.information(self, "Google Drive", "Connect Google Drive first.")
            return

        def load(progress):
            progress(20, f"Loading Drive history for {entry.game_name or entry.appid}...")
            items = self.service.cloud_backups(entry.appid)
            progress(100, "Cloud history loaded")
            return items

        self._run_task(load, lambda items: self._choose_cloud_restore(entry, items), "Loading cloud history...")

    def _choose_cloud_restore(self, entry: SaveEntry, items) -> None:
        self._set_busy(False)
        items = list(items or [])
        if not items:
            QMessageBox.information(self, "Google Drive", "No cloud backups exist for this AppID.")
            return
        labels = [str(item.get("name") or item.get("id")) for item in items]
        picked, ok = QInputDialog.getItem(self, "Restore from Drive", "Choose cloud backup:", labels, 0, False)
        if not ok or not picked:
            return
        item = items[labels.index(picked)]
        answer = QMessageBox.question(
            self,
            "Restore from Drive",
            "The current save will be safety-backed-up before the cloud snapshot replaces it. Continue?",
        )
        if answer != QMessageBox.Yes:
            return

        def task(progress):
            return self.service.restore_from_drive(
                str(item["id"]),
                str(item.get("name") or f"{entry.appid}-cloud.zip"),
                entry.source_root,
                appid=entry.appid,
                progress=lambda p: progress(p, f"Downloading cloud backup... {p}%"),
            )
        self._run_task(task, lambda result: self._restore_finished(entry, result), "Restoring cloud backup...")

    def _backup_all_drive(self) -> None:
        if not self._entries:
            QMessageBox.information(self, "Google Drive", "No saves were found.")
            return
        if not self.service.auth.is_connected():
            QMessageBox.information(self, "Google Drive", "Connect Google Drive first.")
            return
        entries = list(self._entries)

        def task(progress):
            uploaded = []
            total = len(entries)
            for index, entry in enumerate(entries, start=1):
                base = int(((index - 1) / total) * 100)
                span = max(1, int(100 / total))
                archive, cloud = self.service.backup_to_drive(
                    entry,
                    progress=lambda p, b=base, s=span, e=entry: progress(
                        min(99, b + int((p / 100) * s)), f"Uploading {e.game_name or e.appid}..."
                    ),
                )
                uploaded.append((archive, cloud))
            progress(100, "All Drive backups completed")
            return uploaded
        self._run_task(task, self._backup_all_finished, "Backing up all saves to Drive...")

    def _backup_all_finished(self, result) -> None:
        self._set_busy(False)
        self.progress.setValue(100)
        QMessageBox.information(self, "Google Drive", f"Uploaded {len(result or [])} save backup(s).")
        self.refresh_saves()

    def _browse_cloud(self) -> None:
        def task(progress):
            progress(20, "Listing cloud backups...")
            items = self.service.cloud_backups()
            progress(100, "Cloud backups loaded")
            return items
        self._run_task(task, self._cloud_list_finished, "Loading cloud backups...")

    def _cloud_list_finished(self, items) -> None:
        self._set_busy(False)
        items = list(items or [])
        if not items:
            QMessageBox.information(self, "Cloud backups", "No app-created Drive backups were found.")
            return
        lines = []
        for item in items[:100]:
            props = item.get("appProperties") or {}
            lines.append(f"{props.get('gse_appid', '?')} · {item.get('name', '')} · {item.get('size', '?')} bytes")
        QMessageBox.information(self, "Cloud backups", "\n".join(lines))
