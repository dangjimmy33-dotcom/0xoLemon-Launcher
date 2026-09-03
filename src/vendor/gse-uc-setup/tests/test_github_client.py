from gse_autosetup.core.github_client import select_windows_release_asset

def test_prefers_release_windows_7z_over_debug_and_source():
    assets = [
        {"name": "gse-source.zip", "browser_download_url": "s"},
        {"name": "emu-win-debug.7z", "browser_download_url": "d"},
        {"name": "emu-win-release.7z", "browser_download_url": "r"},
        {"name": "emu-linux-release.tar.gz", "browser_download_url": "l"},
    ]
    chosen = select_windows_release_asset(assets)
    assert chosen.name == "emu-win-release.7z"
    assert chosen.download_url == "r"

def test_client_can_target_official_tools_repository():
    from gse_autosetup.core.github_client import GitHubClient
    client = GitHubClient(repo="alex47exe/gse_fork_tools")
    assert client.latest_release_url.endswith("/repos/alex47exe/gse_fork_tools/releases/latest")
