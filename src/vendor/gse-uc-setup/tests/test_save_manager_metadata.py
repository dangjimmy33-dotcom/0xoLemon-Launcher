import json

from gse_autosetup.save_manager.steam_metadata import SteamMetadataCache


def test_cached_metadata_is_used_without_network(tmp_path):
    cache = SteamMetadataCache(tmp_path / 'metadata.json', tmp_path / 'covers')
    cache._save({"2050650": {"name": "Resident Evil 4", "header_image": "https://example.invalid/header.jpg"}})

    class NoNetwork:
        def get_store_metadata(self, _appid):
            raise AssertionError('network must not be used')

    record = cache.resolve(2050650, client=NoNetwork(), download_art=False)
    assert record.name == 'Resident Evil 4'
    assert record.appid == 2050650
