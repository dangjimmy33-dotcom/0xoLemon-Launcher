from gse_autosetup.service import SetupService


def test_service_can_use_bundled_gse_seed_when_online_package_is_unavailable():
    package = SetupService()._bundled_package()
    assert package.version in {"2026_02_16", "embedded", "bundled-seed"}
    assert package.dll("x64").is_file()
    assert package.generator("x86").is_file()


def test_ensure_tools_uses_valid_local_generator_without_release_check(tmp_path):
    from gse_autosetup.core.resources import ResourceManager
    runtime = tmp_path / 'runtime'
    root = runtime / 'embedded' / 'gse_tools'
    exe_dir = root / 'generate_emu_config'
    exe_dir.mkdir(parents=True)
    (exe_dir / 'generate_emu_config.exe').write_bytes(b'MZ')
    (root / 'component.json').write_text('{"tag":"2026_02_16"}', encoding='utf-8')
    rm = ResourceManager(app_dir=tmp_path / 'app', runtime_resources=runtime)
    service = SetupService(resources=rm)

    class NoNetwork:
        def latest_release(self):
            raise AssertionError('release check must not run when local generator is valid')
    service.tools_github = NoNetwork()

    generator_dir, tag = service._ensure_tools()
    assert generator_dir == exe_dir
    assert tag == '2026_02_16'
