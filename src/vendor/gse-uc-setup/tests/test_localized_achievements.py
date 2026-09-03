
import json
from pathlib import Path

from gse_autosetup.core.config_builder import schema_to_achievements, write_basic_settings


def _schema(display, desc, icon="https://cdn/0123456789abcdef0123456789abcdef01234567.jpg",
            gray="https://cdn/fedcba9876543210fedcba9876543210fedcba98.jpg"):
    return {"game": {"availableGameStats": {"achievements": [{
        "name": "ACH_ONE", "displayName": display, "description": desc,
        "hidden": 0, "icon": icon, "icongray": gray,
    }]}}}


def test_multilingual_achievement_maps_and_hash_artwork_names(tmp_path):
    en = _schema("Winner", "Win")
    fr = _schema("Gagnant", "Gagnez")
    ja = _schema("勝者", "勝利する")
    achievements = schema_to_achievements(en, {"english": en, "french": fr, "japanese": ja})
    ach = achievements[0]
    assert ach["displayName"] == {"english": "Winner", "french": "Gagnant", "japanese": "勝者"}
    assert ach["description"] == {"english": "Win", "french": "Gagnez", "japanese": "勝利する"}
    assert ach["hidden"] == 0

    def dl(_url, path):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"x")
        return True

    settings = tmp_path / "steam_settings"
    write_basic_settings(settings, 1, en, "0xoLemon", dl,
                         localized_schemas={"english": en, "french": fr, "japanese": ja})
    data = json.loads((settings / "achievements.json").read_text(encoding="utf-8"))
    assert data[0]["icon"].endswith("0123456789abcdef0123456789abcdef01234567.jpg")
    assert data[0]["icon_gray"].endswith("fedcba9876543210fedcba9876543210fedcba98.jpg")


def test_existing_official_schema_is_preserved_byte_for_byte(tmp_path):
    en = _schema('Winner', 'Win')
    settings = tmp_path / 'steam_settings'
    settings.mkdir()
    canonical_achievements = '[\n  {"hidden": 0, "displayName": {"english": "Winner", "japanese": "勝者"}, "token": "ACH_TOKEN", "name": "ACH_ONE"}\n]\n'
    canonical_stats = '[\n  {"name": "kills", "type": "int", "default": "0", "global": "0"}\n]\n'
    (settings / 'achievements.json').write_text(canonical_achievements, encoding='utf-8')
    (settings / 'stats.json').write_text(canonical_stats, encoding='utf-8')

    downloaded = []
    def dl(url, path):
        downloaded.append((url, path))
        return True

    counts = write_basic_settings(
        settings, 1, en, '0xoLemon', dl,
        localized_schemas={'english': en},
        preserve_existing_schema=True,
    )

    assert (settings / 'achievements.json').read_text(encoding='utf-8') == canonical_achievements
    assert (settings / 'stats.json').read_text(encoding='utf-8') == canonical_stats
    assert downloaded == []
    assert counts == {'achievements': 1, 'stats': 1}


def test_generated_achievement_is_preserved_when_localized_api_data_exists(tmp_path):
    en = _schema("Winner", "Win")
    fr = _schema("Gagnant", "Gagnez")
    ja = _schema("勝者", "勝利する")
    settings = tmp_path / "steam_settings"
    settings.mkdir()
    # Simulate a stale/limited generator artifact: it exists, but only English.
    (settings / "achievements.json").write_text(
        json.dumps([{
            "name": "ACH_ONE",
            "hidden": 0,
            "displayName": {"english": "Winner"},
            "description": {"english": "Win"},
        }], ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    (settings / "supported_languages.txt").write_text("english\n", encoding="utf-8")

    def dl(_url, path):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"x")
        return True

    write_basic_settings(
        settings,
        1,
        en,
        "0xoLemon",
        dl,
        localized_schemas={"english": en, "french": fr, "japanese": ja},
        preserve_existing_achievements=True,
    )

    data = json.loads((settings / "achievements.json").read_text(encoding="utf-8"))
    assert data[0]["displayName"] == {"english": "Winner"}
    assert data[0]["description"] == {"english": "Win"}
    languages = (settings / "supported_languages.txt").read_text(encoding="utf-8").splitlines()
    assert languages == ["english"]
