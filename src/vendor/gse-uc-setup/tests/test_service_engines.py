from pathlib import Path

from gse_autosetup.service import Inputs


def test_inputs_support_gse_and_uc_modes(tmp_path: Path):
    gse = Inputs(appid=1, game_folder=tmp_path, api_key="abcdefgh", engine="gse", gse_variant="coldclient")
    uc = Inputs(appid=1, game_folder=tmp_path, api_key="", engine="uc", uc_spoof_appid=480, steamstub_mode="uc_runtime")
    assert gse.engine == "gse"
    assert gse.gse_variant == "coldclient"
    assert uc.engine == "uc"
    assert uc.uc_spoof_appid == 480
    assert uc.steamstub_mode == "uc_runtime"
