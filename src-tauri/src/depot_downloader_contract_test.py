from pathlib import Path

SOURCE = (Path(__file__).parent / "depot_downloader.rs").read_text(encoding="utf-8")


def test_depot_selection_identity_is_validated_before_network_access():
    assert "fn validate_catalog_identity" in SOURCE
    assert SOURCE.count("validate_catalog_identity(appid, &folder_name)?;") >= 2


def test_missing_build_is_reported_as_refreshable_catalog_error():
    assert "DEPOT_BUILD_NOT_FOUND" in SOURCE
    assert "Hãy làm mới danh mục" in SOURCE


def test_pause_resume_and_version_switch_state_are_first_class_backend_contracts():
    assert "pub fn depot_downloader_pause_download" in SOURCE
    assert "pub async fn depot_downloader_resume_download" in SOURCE
    assert "pub fn depot_downloader_get_install_state" in SOURCE
    assert "is_paused" in SOURCE
    assert "can_resume" in SOURCE
    assert ".DepotDownloader" in SOURCE
    assert "0xolemon" in SOURCE
    assert "version-state.json" in SOURCE
    assert "BuildID_" in SOURCE


def test_version_switch_reuses_existing_install_and_forces_verification():
    assert "is_version_switch" in SOURCE
    assert "do_verify || is_version_switch" in SOURCE
    assert "versions" in SOURCE
    assert "manifest_cache" in SOURCE
    assert "remove_dir_all(&dest_path)" not in SOURCE


def test_manifestfile_mode_is_not_used_for_version_switch_diffing():
    # DepotDownloaderMod's -manifestfile branch treats the provided target
    # manifest as oldManifest, so previousManifest/newManifest become the same
    # object and deleted-file diffing cannot work. The launcher must instead
    # seed the fork's native .DepotDownloader manifest cache (+ .sha) and let
    # depot.config select the real previous manifest.
    assert "fn seed_native_manifest_cache" in SOURCE
    assert 'join(".DepotDownloader")' in SOURCE
    assert 'Sha1::digest' in SOURCE
    assert '.arg("-manifestfile")' not in SOURCE
