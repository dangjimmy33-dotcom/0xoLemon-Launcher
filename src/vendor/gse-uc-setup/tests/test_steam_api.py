import pytest
from gse_autosetup.core.steam_api import SteamApiClient

class FakeResponse:
    status_code = 500
    ok = False
    def json(self): return {}

class FakeSession:
    def get(self, *args, **kwargs): return FakeResponse()

def test_schema_http_error_never_exposes_api_key():
    key = "SECRET_KEY_123456789"
    client = SteamApiClient(key)
    client.session = FakeSession()
    with pytest.raises(Exception) as exc:
        client.get_schema(480)
    assert key not in str(exc.value)
