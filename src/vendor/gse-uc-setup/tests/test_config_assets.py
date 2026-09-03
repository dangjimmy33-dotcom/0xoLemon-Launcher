import json

from gse_autosetup.core.config_builder import write_basic_settings


def test_web_api_achievement_icons_are_written_to_gse_img_folder(tmp_path):
    schema = {"game": {"availableGameStats": {"achievements": [{
        "name": "ACH_ONE",
        "displayName": "One",
        "description": "First",
        "hidden": 0,
        "icon": "https://cdn/icon.jpg",
        "icongray": "https://cdn/gray.jpg",
    }]}}}

    downloaded = []

    def fake_download(url, dest):
        downloaded.append((url, dest))
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(b"IMG")
        return True

    settings = tmp_path / "steam_settings"
    result = write_basic_settings(settings, 480, schema, "Player", fake_download)

    data = json.loads((settings / "achievements.json").read_text(encoding="utf-8"))
    assert result["achievements"] == 1
    assert (settings / "img" / "ACH_ONE.jpg").is_file()
    assert (settings / "img" / "ACH_ONE_gray.jpg").is_file()
    assert data[0]["icon"] == "img/ACH_ONE.jpg"
    assert data[0]["icon_gray"] == "img/ACH_ONE_gray.jpg"
    assert len(downloaded) == 2


def test_counts_web_api_achievement_image_assets():
    from gse_autosetup.core.config_builder import achievement_image_count
    schema = {"game": {"availableGameStats": {"achievements": [
        {"name": "A", "icon": "u1", "icongray": "u2"},
        {"name": "B", "icon": "u3", "icongray": ""},
        {"name": "C"},
    ]}}}
    assert achievement_image_count(schema) == 3
