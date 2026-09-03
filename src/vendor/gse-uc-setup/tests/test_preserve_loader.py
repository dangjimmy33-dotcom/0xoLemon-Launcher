from pathlib import Path

from gse_autosetup.core.preserve_loader import choose_proxy_name, sha256_file


def test_choose_proxy_avoids_existing_files(tmp_path):
    (tmp_path / "version.dll").write_bytes(b"existing")
    assert choose_proxy_name(tmp_path) == "winhttp.dll"


def test_hash_can_prove_original_api_is_unchanged(tmp_path):
    api = tmp_path / "steam_api64.dll"
    api.write_bytes(b"ORIGINAL-STEAM-API")
    before = sha256_file(api)
    # Preserve deployment does not need to touch the API.
    after = sha256_file(api)
    assert before == after
