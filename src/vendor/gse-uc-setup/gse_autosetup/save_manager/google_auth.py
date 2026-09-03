from __future__ import annotations

import json
import os
import sys
from pathlib import Path

from ..core.tool_config import app_directory
from .oauth_store import OAuthCredentialStore

DRIVE_FILE_SCOPE = "https://www.googleapis.com/auth/drive.file"
SCOPES = [DRIVE_FILE_SCOPE]


class GoogleReauthRequired(RuntimeError):
    pass


def packaged_root() -> Path:
    return Path(getattr(sys, "_MEIPASS", app_directory()))


def client_secret_candidates() -> list[Path]:
    app = app_directory()
    packaged = packaged_root()
    return [
        app / "client_secrets.json",
        app / "resources" / "google" / "client_secrets.json",
        packaged / "resources" / "google" / "client_secrets.json",
    ]


def find_client_secrets() -> Path:
    for path in client_secret_candidates():
        if path.is_file():
            return path
    raise FileNotFoundError(
        "Google OAuth client configuration is missing. Put client_secrets.json beside the EXE or in resources\\google."
    )


def default_oauth_store() -> OAuthCredentialStore:
    return OAuthCredentialStore(app_directory() / "data" / "google_oauth.bin")


def credentials_to_dict(creds) -> dict:
    return {
        "token": creds.token,
        "refresh_token": creds.refresh_token,
        "token_uri": creds.token_uri,
        "client_id": creds.client_id,
        "client_secret": creds.client_secret,
        "scopes": list(creds.scopes or SCOPES),
        "expiry": creds.expiry.isoformat() if getattr(creds, "expiry", None) else None,
    }


def credentials_from_dict(info: dict):
    try:
        from google.oauth2.credentials import Credentials
    except ImportError as exc:
        raise RuntimeError("Google Drive support is not installed. Rebuild after installing requirements.txt.") from exc
    clean = dict(info)
    clean.pop("expiry", None)
    creds = Credentials.from_authorized_user_info(clean, scopes=SCOPES)
    expiry = info.get("expiry")
    if expiry:
        from datetime import datetime
        try:
            creds.expiry = datetime.fromisoformat(str(expiry))
        except Exception:
            pass
    return creds


class GoogleAuthManager:
    def __init__(self, store: OAuthCredentialStore | None = None):
        self.store = store or default_oauth_store()

    def connect(self):
        try:
            from google_auth_oauthlib.flow import InstalledAppFlow
        except ImportError as exc:
            raise RuntimeError("google-auth-oauthlib is missing. Rebuild after installing requirements.txt.") from exc
        secrets = find_client_secrets()
        flow = InstalledAppFlow.from_client_secrets_file(str(secrets), scopes=SCOPES)
        creds = flow.run_local_server(
            host="127.0.0.1",
            port=0,
            open_browser=True,
            access_type="offline",
            prompt="consent",
        )
        if not getattr(creds, "refresh_token", None):
            raise GoogleReauthRequired("Google did not return a refresh token. Reconnect and grant offline access.")
        self.store.save(credentials_to_dict(creds))
        return creds

    def get_credentials(self, *, refresh: bool = True):
        info = self.store.load()
        if not info:
            raise GoogleReauthRequired("Google Drive is not connected.")
        creds = credentials_from_dict(info)
        if refresh and (not getattr(creds, "valid", False) or getattr(creds, "expired", False)):
            if not getattr(creds, "refresh_token", None):
                raise GoogleReauthRequired("Google Drive authorization must be renewed.")
            try:
                from google.auth.transport.requests import Request
                creds.refresh(Request())
            except Exception as exc:
                raise GoogleReauthRequired(f"Google Drive authorization refresh failed: {exc}") from exc
            self.store.save(credentials_to_dict(creds))
        return creds

    def is_connected(self) -> bool:
        try:
            return bool(self.store.load().get("refresh_token"))
        except Exception:
            return False

    def disconnect(self) -> None:
        self.store.clear()
