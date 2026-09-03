from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
from typing import Any

SCHEMA_VERSION = 1

COMPONENT_DEFINITIONS: list[dict[str, Any]] = [
    {
        'id': 'gseRegular',
        'label': 'GSE Regular',
        'metadata': 'embedded/gse/component.json',
        'prefixes': ['embedded/gse/regular/'],
        'requiredFiles': [
            'embedded/gse/regular/x86/steam_api.dll',
            'embedded/gse/regular/x64/steam_api64.dll',
        ],
        'license': 'LGPL-3.0-or-later',
    },
    {
        'id': 'gseExperimental',
        'label': 'GSE Experimental',
        'metadata': 'embedded/gse/component.json',
        'prefixes': ['embedded/gse/experimental/'],
        'requiredFiles': [
            'embedded/gse/experimental/x86/steam_api.dll',
            'embedded/gse/experimental/x64/steam_api64.dll',
        ],
        'license': 'upstream component terms',
    },
    {
        'id': 'gseColdClient',
        'label': 'GSE ColdClient',
        'metadata': 'embedded/gse/component.json',
        'prefixes': ['embedded/gse/steamclient_experimental/'],
        'requiredFiles': [
            'embedded/gse/steamclient_experimental/steamclient_loader_x86.exe',
            'embedded/gse/steamclient_experimental/steamclient_loader_x64.exe',
            'embedded/gse/steamclient_experimental/steamclient.dll',
            'embedded/gse/steamclient_experimental/steamclient64.dll',
            'embedded/gse/steamclient_experimental/GameOverlayRenderer.dll',
            'embedded/gse/steamclient_experimental/GameOverlayRenderer64.dll',
        ],
        'license': 'upstream component terms',
    },
    {
        'id': 'gseTools',
        'label': 'GSE Tools / Interface Generator',
        'metadata': 'embedded/gse_tools/component.json',
        'prefixes': ['embedded/gse_tools/'],
        'requiredFiles': [
            'embedded/gse_tools/generate_emu_config/generate_emu_config.exe',
            'embedded/gse_tools/parse_achievements_schema/parse_achievements_schema.exe',
            'embedded/gse_tools/parse_controller_vdf/parse_controller_vdf.exe',
        ],
        'license': None,
    },
    {
        'id': 'ucOnline2',
        'label': 'UC Online2',
        'metadata': 'embedded/uc_online/component.json',
        'prefixes': ['embedded/uc_online/'],
        'requiredFiles': [
            'embedded/uc_online/uc-online2-v1.7.0a-release/x86/steam_api.dll',
            'embedded/uc_online/uc-online2-v1.7.0a-release/x64/steam_api64.dll',
        ],
        'license': None,
    },
    {
        'id': 'runeRegular',
        'label': 'RUNE Regular',
        'metadata': 'embedded/rune/component.json',
        'prefixes': ['embedded/rune/emu/'],
        'requiredFiles': [
            'embedded/rune/emu/steam_api.dll',
            'embedded/rune/emu/steam_api64.dll',
            'embedded/rune/emu/steam_emu.ini',
        ],
        'license': None,
    },
    {
        'id': 'runeSteakClient',
        'label': 'RUNE Steakclient',
        'metadata': 'embedded/rune/component.json',
        'prefixes': ['embedded/rune/steakclient/'],
        'requiredFiles': [
            'embedded/rune/steakclient/steakclient64.dll',
            'embedded/rune/steakclient/winmm.dll',
            'embedded/rune/steakclient/steak_emu.ini',
        ],
        'license': None,
    },
    {
        'id': 'runeSteamClient',
        'label': 'RUNE Steamclient',
        'metadata': 'embedded/rune/component.json',
        'prefixes': ['embedded/rune/steamclient/'],
        'requiredFiles': [
            'embedded/rune/steamclient/x86/steamclient.dll',
            'embedded/rune/steamclient/x86/rune.dll',
            'embedded/rune/steamclient/x86/GameOverlayRenderer.dll',
            'embedded/rune/steamclient/x86/steam_emu.ini',
            'embedded/rune/steamclient/x64/steamclient64.dll',
            'embedded/rune/steamclient/x64/rune64.dll',
            'embedded/rune/steamclient/x64/GameOverlayRenderer64.dll',
            'embedded/rune/steamclient/x64/steam_emu.ini',
        ],
        'license': None,
    },
    {
        'id': 'runeSteamStub',
        'label': 'RUNE SteamStub Patcher',
        'metadata': 'embedded/rune_steamstub/component.json',
        'prefixes': ['embedded/rune_steamstub/'],
        'requiredFiles': [
            'embedded/rune_steamstub/steamstub_x32.dll',
            'embedded/rune_steamstub/steamstub_x64.dll',
        ],
        'license': None,
    },
    {
        'id': 'steamless',
        'label': 'Steamless',
        'metadata': 'embedded/steamless/component.json',
        'prefixes': ['embedded/steamless/'],
        'requiredFiles': ['embedded/steamless/Steamless.CLI.exe'],
        'license': None,
    },
    {
        'id': 'migrateGse',
        'label': 'migrate_gse',
        'metadata': 'embedded/migrate_gse/component.json',
        'prefixes': ['embedded/migrate_gse/'],
        'requiredFiles': ['embedded/migrate_gse/migrate_gse.exe'],
        'license': None,
    },
    {
        'id': 'dinputBridge',
        'label': 'dinput8 Bridge',
        'metadata': None,
        'prefixes': ['embedded/dinput/'],
        'requiredFiles': ['embedded/dinput/dinput8.dll', 'embedded/dinput/dinput8.ini'],
        'license': None,
    },
    {
        'id': 'sevenZip',
        'label': '7-Zip CLI',
        'metadata': None,
        'prefixes': ['7zip/'],
        'requiredFiles': ['7zip/7za.exe', '7zip/7za-license.txt'],
        'license': '7-Zip license',
    },
    {
        'id': 'gseColdClientV1',
        'label': 'GSE ColdClient v1 preserve bridge',
        'metadata': None,
        'prefixes': ['preserve_seed/'],
        'requiredFiles': ['preserve_seed/coldloader.ini', 'preserve_seed/version.ini'],
        'license': None,
    },
]


def normalize_relative_path(raw: str) -> str:
    value = raw.replace('\\', '/')
    p = PurePosixPath(value)
    if p.is_absolute() or any(part in ('', '.', '..') for part in p.parts):
        raise ValueError(f'unsafe relative path: {raw!r}')
    return p.as_posix()


def sha256_file(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open('rb') as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b''):
            hasher.update(chunk)
    return hasher.hexdigest()


def read_metadata(root: Path, relative: str | None) -> dict[str, Any]:
    if not relative:
        return {}
    path = root / normalize_relative_path(relative)
    if not path.is_file():
        return {}
    raw = path.read_text(encoding='utf-8-sig')
    data = json.loads(raw)
    return data if isinstance(data, dict) else {}


def build_manifest(root: Path, package_version: str) -> dict[str, Any]:
    root = root.resolve()
    files: list[dict[str, Any]] = []
    for path in sorted((p for p in root.rglob('*') if p.is_file()), key=lambda p: p.as_posix().lower()):
        rel = normalize_relative_path(path.relative_to(root).as_posix())
        # The launcher deliberately does not import Google OAuth client secrets.
        if rel.startswith('google/'):
            continue
        if rel == 'manifest.json':
            continue
        files.append({
            'relativePath': rel,
            'sizeBytes': path.stat().st_size,
            'sha256': sha256_file(path),
        })

    file_index = {row['relativePath']: row for row in files}
    components: list[dict[str, Any]] = []
    for definition in COMPONENT_DEFINITIONS:
        metadata = read_metadata(root, definition.get('metadata'))
        required = [normalize_relative_path(item) for item in definition['requiredFiles']]
        missing = [item for item in required if item not in file_index]
        component_files = [
            row['relativePath'] for row in files
            if any(row['relativePath'].startswith(prefix) for prefix in definition['prefixes'])
        ]
        components.append({
            'id': definition['id'],
            'label': definition['label'],
            'source': metadata.get('source'),
            'tag': metadata.get('tag'),
            'declaredSha256': metadata.get('sha256'),
            'license': definition.get('license'),
            'requiredFiles': required,
            'fileCount': len(component_files),
            'files': component_files,
            'missingRequiredFiles': missing,
        })

    return {
        'schemaVersion': SCHEMA_VERSION,
        'packageVersion': package_version,
        'sourceSnapshot': 'GSE_UC_Setup_V1_8_2 user-supplied handoff',
        'integrityModel': 'sha256-per-file',
        'provenanceModel': 'user-supplied-snapshot-not-reproducible-build-proof',
        'components': components,
        'files': files,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('resource_root', type=Path)
    parser.add_argument('--package-version', default='1.8.3')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    manifest = build_manifest(args.resource_root, args.package_version)
    output = args.output or args.resource_root / 'manifest.json'
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    print(f'wrote {output} with {len(manifest["files"])} files / {len(manifest["components"])} components')
    missing = {
        c['id']: c['missingRequiredFiles'] for c in manifest['components'] if c['missingRequiredFiles']
    }
    if missing:
        print(json.dumps({'missingRequiredFiles': missing}, indent=2))
        return 2
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
