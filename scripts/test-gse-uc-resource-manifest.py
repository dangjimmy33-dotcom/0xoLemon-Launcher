from __future__ import annotations

import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

MODULE = Path(__file__).with_name('build-gse-uc-resource-manifest.py')


def load_module():
    spec = importlib.util.spec_from_file_location('gse_manifest', MODULE)
    if spec is None or spec.loader is None:
        raise RuntimeError('cannot load manifest generator')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ManifestGeneratorTests(unittest.TestCase):
    def test_component_manifest_is_deterministic_and_hashes_files(self):
        mod = load_module()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            component = root / 'embedded' / 'gse'
            (component / 'regular' / 'x64').mkdir(parents=True)
            payload = b'goldberg-test-runtime'
            dll = component / 'regular' / 'x64' / 'steam_api64.dll'
            dll.write_bytes(payload)
            (component / 'component.json').write_text(
                json.dumps({'source': 'example/gse', 'tag': 'v1'}), encoding='utf-8'
            )

            first = mod.build_manifest(root, package_version='1.8.3')
            second = mod.build_manifest(root, package_version='1.8.3')
            self.assertEqual(first, second)
            self.assertEqual(first['schemaVersion'], 1)
            self.assertEqual(first['packageVersion'], '1.8.3')
            file_row = next(
                row for row in first['files']
                if row['relativePath'] == 'embedded/gse/regular/x64/steam_api64.dll'
            )
            self.assertEqual(file_row['sha256'], hashlib.sha256(payload).hexdigest())
            self.assertEqual(file_row['sizeBytes'], len(payload))
            gse = next(c for c in first['components'] if c['id'] == 'gseRegular')
            self.assertEqual(gse['source'], 'example/gse')
            self.assertEqual(gse['tag'], 'v1')

    def test_manifest_paths_cannot_escape_resource_root(self):
        mod = load_module()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            root.mkdir(exist_ok=True)
            with self.assertRaises(ValueError):
                mod.normalize_relative_path('../escape.dll')
            with self.assertRaises(ValueError):
                mod.normalize_relative_path('/absolute.dll')
            self.assertEqual(mod.normalize_relative_path('embedded/gse/a.dll'), 'embedded/gse/a.dll')

    def test_component_definitions_cover_launcher_modes(self):
        mod = load_module()
        ids = {row['id'] for row in mod.COMPONENT_DEFINITIONS}
        self.assertTrue({
            'gseRegular', 'gseExperimental', 'gseColdClient', 'gseTools',
            'ucOnline2', 'runeRegular', 'runeSteamStub', 'steamless',
            'migrateGse', 'dinputBridge', 'sevenZip'
        }.issubset(ids))


if __name__ == '__main__':
    unittest.main()
