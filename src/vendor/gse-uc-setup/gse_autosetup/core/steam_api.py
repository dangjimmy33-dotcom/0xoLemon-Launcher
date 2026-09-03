from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Callable

import requests

from .models import GameMetadata


# Steam "API language code" values. GetSchemaForGame accepts these names
# (english, french, schinese, etc.), not the shorter Web API UI codes.
STEAM_PLATFORM_LANGUAGES = (
    "english", "arabic", "bulgarian", "schinese", "tchinese", "czech",
    "danish", "dutch", "finnish", "french", "german", "greek", "hungarian",
    "indonesian", "italian", "japanese", "koreana", "malay", "norwegian",
    "polish", "portuguese", "brazilian", "romanian", "russian", "spanish",
    "latam", "swedish", "thai", "turkish", "ukrainian", "vietnamese",
)


class SteamApiClient:
    def __init__(self, api_key: str, timeout: int = 20):
        self.api_key = api_key.strip()
        self.timeout = timeout
        self.session = requests.Session()
        self.session.headers.update({"User-Agent": "GSE-UC-Setup/1.8.3"})

    def _get_schema_with_session(self, appid: int, language: str, session: requests.Session) -> dict:
        if not self.api_key:
            raise ValueError("Steam Web API key is required.")
        try:
            response = session.get(
                "https://api.steampowered.com/ISteamUserStats/GetSchemaForGame/v2/",
                params={"key": self.api_key, "appid": int(appid), "l": str(language or "english")},
                timeout=self.timeout,
            )
        except requests.RequestException:
            # Never surface a requests exception containing the full URL/API key.
            raise RuntimeError("Steam Web API request failed due to a network error.") from None
        if response.status_code in (401, 403):
            raise ValueError("Steam Web API key was rejected.")
        if not getattr(response, "ok", 200 <= response.status_code < 300):
            raise RuntimeError(f"Steam Web API request failed (HTTP {response.status_code}).")
        try:
            data = response.json()
        except Exception:
            raise RuntimeError("Steam Web API returned invalid JSON.") from None
        if "game" not in data:
            raise ValueError("Steam returned no schema for this AppID.")
        return data

    def get_schema(self, appid: int, language: str = "english") -> dict:
        return self._get_schema_with_session(appid, language, self.session)

    def get_localized_schemas(
        self,
        appid: int,
        languages: list[str] | tuple[str, ...] | None = None,
        progress: Callable[[int, int, str], None] | None = None,
        max_workers: int = 6,
    ) -> dict[str, dict]:
        """Fetch localized schemas without making one slow serial request per language.

        English is always requested. Individual language failures are non-fatal; callers
        can safely fall back to English for missing strings.
        """
        ordered: list[str] = []
        for lang in (languages or STEAM_PLATFORM_LANGUAGES):
            lang = str(lang).strip().lower()
            if lang and lang not in ordered:
                ordered.append(lang)
        if "english" not in ordered:
            ordered.insert(0, "english")

        results: dict[str, dict] = {}
        total = len(ordered)

        def fetch_one(lang: str) -> tuple[str, dict]:
            session = requests.Session()
            session.headers.update({"User-Agent": "GSE-UC-Setup/1.8.3"})
            try:
                return lang, self._get_schema_with_session(appid, lang, session)
            finally:
                session.close()

        workers = max(1, min(int(max_workers), 8, total))
        with ThreadPoolExecutor(max_workers=workers, thread_name_prefix="steam-schema") as pool:
            futures = {pool.submit(fetch_one, lang): lang for lang in ordered}
            done = 0
            for future in as_completed(futures):
                lang = futures[future]
                done += 1
                try:
                    got_lang, schema = future.result()
                    results[got_lang] = schema
                except ValueError:
                    # Authentication/AppID errors are meaningful and should not be hidden.
                    if lang == "english":
                        raise
                except Exception:
                    # Some games do not expose every localization. English remains fallback.
                    pass
                if progress:
                    progress(done, total, lang)

        if "english" not in results:
            # Guarantee a usable base schema or raise the same sanitized error path.
            results["english"] = self.get_schema(appid, "english")
        return results

    def get_store_metadata(self, appid: int) -> GameMetadata:
        try:
            response = self.session.get(
                "https://store.steampowered.com/api/appdetails",
                params={"appids": int(appid), "l": "english"},
                timeout=self.timeout,
            )
            response.raise_for_status()
            data = response.json().get(str(appid), {})
        except requests.RequestException:
            raise RuntimeError("Steam Store metadata request failed.") from None
        details = data.get("data") if data.get("success") else None
        if not details:
            return GameMetadata(appid=appid, name=f"Steam App {appid}")
        return GameMetadata(
            appid=appid,
            name=str(details.get("name") or f"Steam App {appid}"),
            header_image=details.get("header_image"),
        )

    def get_dlcs(self, appid: int) -> list[tuple[int, str]]:
        """Fetch list of (dlc_appid, dlc_name) for a given game AppID from Steam Store API."""
        try:
            response = self.session.get(
                "https://store.steampowered.com/api/appdetails",
                params={"appids": int(appid), "l": "english"},
                timeout=self.timeout,
            )
            if not response.ok:
                return []
            data = response.json().get(str(appid), {})
            if not data.get("success"):
                return []
            dlc_ids = data.get("data", {}).get("dlc", [])
            if not dlc_ids:
                return []

            results: list[tuple[int, str]] = []

            def _fetch_name(d_id: int) -> tuple[int, str]:
                try:
                    r = self.session.get(
                        "https://store.steampowered.com/api/appdetails",
                        params={"appids": int(d_id), "l": "english", "filters": "basic"},
                        timeout=10,
                    )
                    if r.ok:
                        d_data = r.json().get(str(d_id), {})
                        if d_data.get("success"):
                            return int(d_id), str(d_data.get("data", {}).get("name") or f"DLC {d_id}")
                except Exception:
                    pass
                return int(d_id), f"DLC {d_id}"

            with ThreadPoolExecutor(max_workers=min(8, len(dlc_ids))) as pool:
                futures = [pool.submit(_fetch_name, d) for d in dlc_ids]
                for fut in as_completed(futures):
                    try:
                        results.append(fut.result())
                    except Exception:
                        pass

            results.sort(key=lambda item: item[0])
            return results
        except Exception:
            return []

    def download_file(
        self,
        url: str,
        destination: Path,
        progress: Callable[[int, int], None] | None = None,
    ) -> bool:
        try:
            destination.parent.mkdir(parents=True, exist_ok=True)
            with self.session.get(url, stream=True, timeout=(10, 45)) as response:
                response.raise_for_status()
                total = int(response.headers.get("Content-Length") or 0)
                done = 0
                tmp = destination.with_suffix(destination.suffix + ".part")
                with tmp.open("wb") as f:
                    for chunk in response.iter_content(128 * 1024):
                        if not chunk:
                            continue
                        f.write(chunk)
                        done += len(chunk)
                        if progress:
                            progress(done, total)
                tmp.replace(destination)
            return True
        except Exception:
            return False
