from gse_autosetup.core.tool_config import ToolConfig, load_config, save_config


def test_save_manager_preferences_roundtrip(tmp_path):
    path = tmp_path / 'config.ini'
    cfg = ToolConfig(last_top_tab='save_manager', save_manager_root=r'D:\GSE Saves')
    save_config(cfg, path)
    loaded = load_config(path)
    assert loaded.last_top_tab == 'save_manager'
    assert loaded.save_manager_root == r'D:\GSE Saves'
