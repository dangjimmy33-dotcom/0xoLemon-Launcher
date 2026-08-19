from pathlib import Path

ROOT = Path(__file__).resolve().parent
LUA_SOURCES = (ROOT / 'lua_sources.rs').read_text(encoding='utf-8')
LUA_LIVE = (ROOT / 'lua_live.rs').read_text(encoding='utf-8')


def fn_body(text: str, name: str) -> str:
    for marker in (f'pub(crate) fn {name}', f'fn {name}'):
        start = text.find(marker)
        if start >= 0:
            break
    assert start >= 0, f'missing function {name}'
    brace = text.find('{', start)
    depth = 0
    for i in range(brace, len(text)):
        ch = text[i]
        if ch == '{':
            depth += 1
        elif ch == '}':
            depth -= 1
            if depth == 0:
                return text[start:i + 1]
    raise AssertionError(f'unclosed function {name}')


def test_hubcap_install_uses_the_real_combined_lua_manifest_endpoint():
    body = fn_body(LUA_SOURCES, 'fetch_hubcap_package')
    assert '/api/v1/status/' not in body
    assert '/api/v1/manifest/{appid}' in body
    assert '/api/v1/generate/appmanifest/' not in body


def test_hubcap_picker_probe_is_local_only():
    body = fn_body(LUA_SOURCES, 'probe_hubcap_source')
    assert 'hubcap_get(' not in body


def test_hubcap_archive_is_provider_tolerant():
    body = fn_body(LUA_SOURCES, 'package_from_archive')
    assert 'lua_candidates' in body
    assert 'inspect_provider_lua(appid' in body
    assert 'canonicalize_lua(appid, &candidate)' not in body


def test_hubcap_keeps_atomic_bundle_requirement():
    body = fn_body(LUA_SOURCES, 'package_from_archive')
    assert 'LuaPackageProvider::Hubcap' in body
    assert 'same-provider bundle is missing its paired manifest' in body


def test_launcher_quota_gate_does_not_block_hubcap_downloads():
    body = fn_body(LUA_LIVE, 'install_live_blocking')
    assert 'reserve_lua_add(' not in body
    assert 'complete_lua_add(' not in body
    assert 'fail_lua_add(' not in body


def test_openlua_only_runs_launcher_guards_in_top_level_openlua_document():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert '.initialization_script(&inject_js)' in body or '.initialization_script(inject_js.clone())' in body
    assert 'initialization_script_for_all_frames' not in body
    origin_guard = "var IS_OPENLUA = host === 'openlua.cloud' || host.endsWith('.openlua.cloud');"
    reject_foreign = 'if (!IS_OPENLUA) return;'
    reject_subframe = 'if (!IS_TOP) return;'
    assert origin_guard in body
    assert reject_foreign in body
    assert reject_subframe in body
    assert body.find(reject_foreign) < body.find("window.addEventListener(type, lockUserPointer, true)")
    assert body.find(reject_subframe) < body.find("window.addEventListener(type, lockUserPointer, true)")


def test_openlua_parent_can_still_find_manual_ad_and_captcha_frames():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert 'challenges.cloudflare.com' in body
    assert 'google.com/recaptcha' in body
    assert 'recaptcha.net' in body
    assert 'hcaptcha.com' in body
    assert 'findBlockingAdTarget' in body
    assert 'findCloudflareTarget' in body


def test_openlua_does_not_monkeypatch_core_browser_apis():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert 'window.fetch =' not in body
    assert 'XMLHttpRequest.prototype.send =' not in body
    assert 'window.URL.createObjectURL =' not in body
    assert 'Notification.requestPermission =' not in body


def test_openlua_verification_submit_is_single_shot_and_gated():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert 'verificationSubmitted' in body
    assert "t.includes('complete verification')" not in body
    assert "enabledActionButton('complete verification')" in body


def test_openlua_allows_only_manual_interstitial_actions_and_guides_to_them():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert 'isAdActionControl' in body
    assert 'frameLooksLikeAdOrInterstitial' in body
    assert 'findBlockingAdTarget' in body
    assert 'var blockingAd = findBlockingAdTarget();' in body



def test_openlua_detects_blocking_ad_modal_and_icon_only_close_without_unlocking_ad_cta():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert 'looksLikeBlockingAdModal' in body
    assert 'isIconOnlyDismissControl' in body
    assert 'claim your bonus' in body.lower()
    assert "claim|bonus" not in body[body.find('function isAdActionControl'):body.find('function frameLooksLikeAdOrInterstitial')]


def test_openlua_guidance_runs_on_a_periodic_timer_for_late_ad_dismiss_controls():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert 'guideHeartbeat' in body
    assert 'guideToManualTarget();' in body

def test_openlua_uses_native_download_hook():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert '.on_download(' in body
    assert 'DownloadEvent::Requested' in body
    assert 'DownloadEvent::Finished' in body
    assert 'inspect_provider_lua(appid, &text).is_ok()' in body


def test_opening_lua_shop_does_not_ping_hubcap_usage():
    body = fn_body(LUA_SOURCES, 'get_lua_source_settings_internal')
    assert 'refresh_hubcap_state_blocking' not in body



def test_luatools_dynamic_sources_are_strictly_namespaced():
    for variant, cache in [
        ('Luie', 'luie'),
        ('TwentyTwoCloud', 'depotbox'),
        ('Skyflare', 'skyflare'),
    ]:
        assert f'{variant},' in LUA_SOURCES
        assert f'Self::{variant} => "{cache}"' in LUA_SOURCES
    assert '(Self::Luie, LuaPackageProvider::Luie)' in LUA_SOURCES
    assert '(Self::TwentyTwoCloud, LuaPackageProvider::TwentyTwoCloud)' in LUA_SOURCES
    assert '(Self::Skyflare, LuaPackageProvider::Skyflare)' in LUA_SOURCES



def test_dynamic_payload_classification_is_content_based():
    body = fn_body(LUA_SOURCES, 'package_from_luatools_direct_payload')
    assert 'bytes.starts_with(b"PK\\x03\\x04")' in body
    assert 'inspect_provider_lua(appid, &source)' in body
    assert 'manifests: Vec::new()' in body
    assert 'build_canonical_archive' in body


def test_openlua_download_state_rearms_after_verification():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert 'downloadGeneration' in body
    assert 'downloadSubmitted = false' in body
    assert 'verificationSubmitted = false' in body
    assert 'lastDownloadButton' in body
    assert "enabledActionButton('complete verification')" in body
    assert 'downloadGeneration++' in body



def test_hybrid_luatools_sources_install_locked_only_when_user_selects_locked():
    live_body = fn_body(LUA_LIVE, 'install_live_blocking')
    assert 'adaptive_luatools_locked' not in live_body
    locked_body = fn_body(LUA_LIVE, 'install_locked_source_blocking')
    assert 'selected_source.supports_locked()' in locked_body
    assert 'has_manifest_pin(&source)' in locked_body
    assert 'package.manifests.is_empty()' in locked_body
    assert 'atomic_write_path(&path, source.as_bytes())' in locked_body
    assert 'state.channel = LuaGameChannel::Locked' in locked_body


def test_openlua_successful_turnstile_does_not_block_download_rearm():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert 'cf-turnstile-response' in body
    assert 'turnstileHasResponse' in body
    assert 'if (turnstileHasResponse()) return null;' in body


def test_luatools_dynamic_sources_use_direct_manifest_discovery_not_localhost_bridge():
    assert 'LUATOOLS_MANIFEST_BACKEND' in LUA_SOURCES
    assert 'check_apis?appid={appid}' in LUA_SOURCES
    assert 'LUATOOLS_MANIFEST_USER_AGENT' in LUA_SOURCES
    body = fn_body(LUA_SOURCES, 'probe_luatools_direct_sources')
    assert '127.0.0.1:6767' not in body


def test_luatools_download_uses_authenticated_direct_api_and_pkce():
    assert 'LUATOOLS_API_BASE' in LUA_SOURCES
    assert '/api/manifest/download?appid={appid}&source=' in LUA_SOURCES
    assert 'code_challenge_method=s256' in LUA_SOURCES
    assert 'LUATOOLS_SUPABASE_ANON_KEY' in LUA_SOURCES
    assert 'AUTHORIZATION' in LUA_SOURCES
    assert 'fetch_luatools_direct_package' in LUA_SOURCES


def test_luatools_direct_sources_do_not_write_to_steam_before_launcher_commit():
    live_body = fn_body(LUA_LIVE, 'install_live_blocking')
    assert 'luatools_side_effect_source' not in live_body
    assert 'bridge_rollback' not in live_body
    assert 'fetch_luatools_direct_package' in LUA_LIVE


def test_openlua_recognizes_target_game_from_url_and_rearms_after_blocker_clears():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert 'function pageMatchesTargetGame()' in body
    assert "url.searchParams.get('app') === APPID" in body
    assert 'var blockerWasVisible = false;' in body
    assert 'if (blockerWasVisible)' in body
    assert 'downloadSubmitted = false;' in body
    assert 'lastDownloadButton = null;' in body


def test_luatools_discovery_failure_isolates_luie_from_dynamic_source_outages():
    fallback = fn_body(LUA_SOURCES, 'luatools_known_stable_wire_name')
    assert 'LuaSourceProvider::Luie => Some("Luie")' in fallback
    body = fn_body(LUA_SOURCES, 'probe_luatools_direct_source')
    assert 'luatools_known_stable_wire_name(provider).is_some()' in body
    assert 'candidate.on_demand = true;' in body
    assert 'LUATOOLS_DISCOVERY_DEFERRED' in body
    resolver = fn_body(LUA_SOURCES, 'luatools_exact_download_source_name')
    assert 'luatools_known_stable_wire_name(provider)' in resolver


def test_luatools_download_requires_exact_wire_name_from_discovery():
    body = fn_body(LUA_SOURCES, 'fetch_luatools_direct_package')
    assert 'luatools_exact_download_source_name' in body
    assert 'luatools_resolved_download_source_names' not in body
    assert 'luatools_direct_download_bytes' in body
    resolver = fn_body(LUA_SOURCES, 'luatools_exact_download_source_name')
    assert 'LUATOOLS_SOURCE_NOT_AVAILABLE' in resolver
    assert 'luatools_match_discovered_source' in resolver


def test_openlua_enabled_download_button_can_outlive_a_stale_turnstile_iframe():
    body = fn_body(LUA_SOURCES, 'fetch_openlua_package')
    assert 'function findDownloadActionButton()' in body
    assert 'var readyDownloadButton = findDownloadActionButton();' in body
    assert '(challengeNow && !readyDownloadButton)' in body



def test_hybrid_sources_and_skyflare_live_only_capabilities_are_explicit():
    supports_live = fn_body(LUA_SOURCES, 'supports_live')
    supports_locked = fn_body(LUA_SOURCES, 'supports_locked')
    assert 'Self::Luie' in supports_live and 'Self::Luie' not in supports_locked
    assert 'Self::TwentyTwoCloud' in supports_live and 'Self::TwentyTwoCloud' in supports_locked
    assert 'Self::Ryuu' in supports_live and 'Self::Ryuu' in supports_locked
    assert 'Self::Skyflare' in supports_live and 'Self::Skyflare' not in supports_locked

    live_body = fn_body(LUA_LIVE, 'install_live_blocking')
    assert 'adaptive_luatools_locked' not in live_body
    locked_body = fn_body(LUA_LIVE, 'install_locked_source_blocking')
    assert 'LOCKED_SOURCE_REQUIRES_MANIFEST_PACKAGE' in locked_body
    assert 'snapshot_installed_source' in locked_body


def test_luatools_download_does_not_retry_guessed_aliases_after_unknown_source():
    body = fn_body(LUA_SOURCES, 'luatools_direct_download_bytes')
    assert 'source_name: &str' in body
    assert 'for source_name in source_names' not in body
    assert 'last_unknown_source' not in body


def test_luatools_rejected_cached_wire_name_refreshes_discovery_once_without_guessing():
    body = fn_body(LUA_SOURCES, 'fetch_luatools_direct_package')
    assert 'LUATOOLS_SOURCE_KEY_REJECTED' in body
    assert 'clear_luatools_wire_name' in body
    assert 'luatools_exact_download_source_name' in body
    assert 'luatools_source_aliases' not in body


def test_hybrid_pinned_packages_never_borrow_manifests_from_other_sources():
    body = fn_body(LUA_SOURCES, 'package_from_archive')
    assert 'LuaPackageProvider::TwentyTwoCloud' in body
    assert 'same-provider bundle is missing' in body


def test_luatools_discovery_matches_official_client_redirect_and_exact_wire_name_contract():
    client = fn_body(LUA_SOURCES, 'luatools_manifest_client')
    assert 'Policy::limited' in client
    assert 'Policy::none' not in client
    available = fn_body(LUA_SOURCES, 'luatools_direct_source_available')
    assert 'luatools_match_discovered_source' in available
    resolver = fn_body(LUA_SOURCES, 'luatools_exact_download_source_name')
    assert 'cached_luatools_wire_name' in resolver
    assert 'luatools_direct_available_source_entries' in resolver
    assert 'cache_luatools_wire_name' in resolver


def test_hubcap_daily_quota_defaults_to_25_and_launcher_quota_gate_is_removed():
    daily = fn_body(LUA_SOURCES, 'daily_usage_bucket')
    assert 'HUBCAP_FREE_DAILY_LIMIT' in daily
    live = fn_body(LUA_LIVE, 'install_live_blocking')
    assert 'reserve_lua_add(' not in live
    assert 'complete_lua_add(' not in live
    assert 'fail_lua_add(' not in live



def test_ryuu_uses_official_authenticated_api_and_is_hybrid():
    assert 'const RYUU_BASE: &str = "https://generator.ryuu.lol";' in LUA_SOURCES
    supports_live = fn_body(LUA_SOURCES, 'supports_live')
    supports_locked = fn_body(LUA_SOURCES, 'supports_locked')
    assert 'Self::Ryuu' in supports_live
    assert 'Self::Ryuu' in supports_locked
    url = fn_body(LUA_SOURCES, 'ryuu_url')
    assert '/api/download/{appid}' in url
    fetch = fn_body(LUA_SOURCES, 'fetch_ryuu_package')
    assert 'X-Auth-Key' in fetch
    assert 'decrypt_ryuu_key' in fetch
    assert 'package_from_archive' in fetch


def test_depotbox_replaces_twenty_two_download_path_without_luatools_proxy():
    assert 'const DEPOTBOX_BASE: &str = "https://depotbox.org";' in LUA_SOURCES
    probe = fn_body(LUA_SOURCES, 'probe_depotbox_source')
    assert 'LuaSourceProvider::TwentyTwoCloud' in probe
    assert 'candidate.on_demand = true' in probe
    fetch = fn_body(LUA_SOURCES, 'fetch_depotbox_package')
    assert 'decrypt_depotbox_key' in fetch
    assert 'fetch_depotbox_api_package' in fetch
    assert 'fetch_depotbox_web_package' in fetch
    assert 'LUATOOLS_API_BASE' not in fetch
    live = LUA_LIVE
    assert 'fetch_depotbox_package' in live


def test_skyflare_uses_direct_skyapi_github_source_and_is_not_luatools_dynamic():
    assert 'const SKYFLARE_GITHUB_CONTENTS_BASE' in LUA_SOURCES
    assert 'https://api.github.com/repos/skyflarefox/Skyapi/contents' in LUA_SOURCES
    probe_all = fn_body(LUA_SOURCES, 'probe_luatools_direct_sources')
    assert 'LuaSourceProvider::Skyflare' not in probe_all
    dispatcher = fn_body(LUA_SOURCES, 'probe_source')
    assert 'LuaSourceProvider::TwentyTwoCloud => probe_depotbox_source' in dispatcher
    assert 'LuaSourceProvider::Skyflare => probe_skyflare_source' in dispatcher
    fetch = fn_body(LUA_SOURCES, 'fetch_skyflare_package')
    assert 'SKYFLARE_GITHUB_CONTENTS_BASE' in fetch
    assert 'download_github_contents_raw_to_part' in fetch
    assert 'raw.githubusercontent.com' not in fetch
    assert 'package_from_archive' in fetch
    assert 'LuaPackageProvider::Skyflare' in fetch

def test_depotbox_is_part_of_source_scan_after_rebrand():
    body = fn_body(LUA_SOURCES, 'scan_lua_sources_blocking')
    provider_block = body[body.find('let providers = ['):body.find('let appid = request.appid;')]
    assert 'LuaSourceProvider::TwentyTwoCloud' in provider_block
    assert 'probe_luatools_direct_sources(app, appid)' in body


def test_channel_selection_is_authoritative_for_hybrid_sources():
    body = fn_body(LUA_LIVE, 'install_live_blocking')
    assert 'adaptive_luatools_locked' not in body
    assert 'LuaGameChannel::Live' in body


def test_locked_source_install_fetches_same_provider_package_directly():
    body = fn_body(LUA_LIVE, 'install_locked_source_blocking')
    assert 'selected_source.supports_locked()' in body
    assert 'fetch_selected_live_source(app, request.appid, selected_source, None)' in body
    assert 'LOCKED_SOURCE_REQUIRES_MANIFEST_PACKAGE' in body
    assert 'snapshot_installed_source' in body
    assert 'atomic_write_path(&path, source.as_bytes())' in body


def test_depotbox_optional_api_key_is_encrypted_and_exposed_as_masked_state():
    assert 'encrypted_depotbox_key' in LUA_SOURCES
    assert 'decrypt_depotbox_key' in LUA_SOURCES
    assert 'save_depotbox_api_key' in LUA_SOURCES
    assert 'clear_depotbox_api_key' in LUA_SOURCES
    assert 'depotbox_configured' in LUA_SOURCES
    assert 'depotbox_key' in LUA_SOURCES


def test_depotbox_paid_mode_uses_documented_async_api_and_header_key():
    fetch = fn_body(LUA_SOURCES, 'fetch_depotbox_api_package')
    assert '/api/lua' in fetch
    assert '/api/download' in fetch
    assert '/api/status/' in fetch
    assert 'X-API-Key' in fetch
    assert 'download_link' in fetch
    assert 'depotbox_api_url' in fetch
    url_guard = fn_body(LUA_SOURCES, 'depotbox_api_url')
    assert 'DEPOTBOX_BASE' in url_guard
    assert 'depotbox.org' in url_guard


def test_depotbox_free_mode_automates_search_selection_and_requested_download_without_solving_captcha():
    fetch = fn_body(LUA_SOURCES, 'fetch_depotbox_web_package')
    assert '.on_download(' in fetch
    assert '.initialization_script(&inject_js)' in fetch
    assert '.resizable(false)' in fetch
    assert '.maximizable(false)' in fetch
    assert "var APPID = '__APPID__';" in fetch
    assert "var WANT_LOCKED = __LOCKED__;" in fetch
    assert 'findSearchInput' in fetch
    assert 'findSearchButton' in fetch
    assert 'findTargetGame' in fetch
    assert 'findRequestedDownload' in fetch
    assert 'download .lua' in fetch
    assert 'download .zip' in fetch
    assert 'isVerificationActive' in fetch
    assert 'verification widgets are manual' in fetch
    assert 'depotbox_package_from_bytes' in fetch
    classify = fn_body(LUA_SOURCES, 'depotbox_package_from_bytes')
    assert 'DEPOTBOX_LIVE_REQUIRES_LUA_DOWNLOAD' in classify
    assert 'DEPOTBOX_LOCKED_REQUIRES_ZIP_DOWNLOAD' in classify


def test_depotbox_channel_is_passed_from_live_and_locked_install_paths():
    live_fetch = fn_body(LUA_LIVE, 'fetch_selected_live_source')
    assert 'fetch_depotbox_package(app, appid, false)' in live_fetch
    locked = fn_body(LUA_LIVE, 'install_locked_source_blocking')
    assert 'fetch_depotbox_package(app, request.appid, true)' in locked


def test_depotbox_commands_are_registered_in_tauri_invoke_acl():
    lib = (ROOT / 'lib.rs').read_text(encoding='utf-8')
    acl = (ROOT.parent / 'permissions' / 'allow-all.json').read_text(encoding='utf-8')
    for command in ('save_depotbox_api_key', 'clear_depotbox_api_key'):
        assert f'lua_sources::{command}' in lib
        assert f'"{command}"' in acl


def test_public_github_sources_are_on_demand_without_picker_preflight_requests():
    sushi = fn_body(LUA_SOURCES, 'probe_sushi_source')
    skyflare = fn_body(LUA_SOURCES, 'probe_skyflare_source')
    for body in (sushi, skyflare):
        assert 'url_exists(' not in body
        assert 'candidate.on_demand = true' in body
        assert 'candidate.variant = Some(' in body


def test_depotbox_free_web_automatically_clicks_the_channel_already_selected_in_launcher():
    fetch = fn_body(LUA_SOURCES, 'fetch_depotbox_web_package')
    assert 'Launcher already knows the requested channel' in fetch
    assert "var WANT_LOCKED = __LOCKED__;" in fetch
    assert "var downloadPhase = 'idle';" in fetch
    assert "downloadPhase = 'waiting-after-click';" in fetch
    assert 'try { download.click(); }' in fetch
    assert 'manualDownloadChoice' not in fetch
    assert 'e.isTrusted' not in fetch
    # Upstream install paths select the channel before DepotBox opens.
    assert 'fetch_depotbox_package(app, appid, false)' in fn_body(LUA_LIVE, 'fetch_selected_live_source')
    assert 'fetch_depotbox_package(app, request.appid, true)' in fn_body(LUA_LIVE, 'install_locked_source_blocking')


def test_depotbox_does_not_click_the_green_ready_button_and_captures_browser_generated_payload():
    fetch = fn_body(LUA_SOURCES, 'fetch_depotbox_web_package')
    assert 'download ready' in fetch.lower()
    assert 'generationHasStarted' in fetch
    assert 'finalDownload.click()' not in fetch
    assert 'function findFinalDownload()' not in fetch
    assert 'window.__OXO_DEPOTBOX_CAPTURE' in fetch
    assert 'window.URL.createObjectURL' in fetch
    assert 'window.fetch =' in fetch
    assert 'HTMLAnchorElement.prototype.click' in fetch
    assert 'TcpListener::bind("127.0.0.1:0")' in fetch
    assert 'Access-Control-Allow-Private-Network: true' in fetch
    assert 'captureDepotboxPayload' in fetch
    assert "window.fetch =" in fetch
    assert 'DEPOTBOX_BROWSER_CAPTURED' in fetch


def test_depotbox_native_download_reports_requested_finished_and_failure_instead_of_silent_timeout():
    fetch = fn_body(LUA_SOURCES, 'fetch_depotbox_web_package')
    assert 'DepotBox download requested:' in fetch
    assert 'DepotBox download finished:' in fetch
    assert 'DEPOTBOX_DOWNLOAD_FAILED' in fetch
    assert 'channel::<Result<Vec<u8>, String>>()' in fetch


def test_depotbox_free_download_uses_real_lua_or_zip_extension_for_native_destination():
    fetch = fn_body(LUA_SOURCES, 'fetch_depotbox_web_package')
    assert 'let destination_name = if locked {' in fetch
    assert 'format!("{appid}.zip")' in fetch
    assert 'format!("{appid}.lua")' in fetch
    assert '.depotbox-download.bin' not in fetch


def test_depotbox_can_recover_completed_temp_file_if_finished_callback_is_missed():
    fetch = fn_body(LUA_SOURCES, 'fetch_depotbox_web_package')
    assert 'last_seen_download_size' in fetch
    assert 'fs::metadata(&destination_path)' in fetch
    assert 'DepotBox download recovered from destination' in fetch



def test_depotbox_atomic_stop_store_uses_rust_atomicbool_argument_order():
    fetch = fn_body(LUA_SOURCES, 'fetch_depotbox_web_package')
    bad = 'browser_stop.store(std::sync::atomic::Ordering::Relaxed, true)'
    good = 'browser_stop.store(true, std::sync::atomic::Ordering::Relaxed)'
    assert bad not in fetch
    assert fetch.count(good) >= 1

def test_skyflare_and_sushi_download_through_github_contents_api_not_raw_host():
    assert 'const SKYFLARE_GITHUB_CONTENTS_BASE' in LUA_SOURCES
    assert 'const SUSHI_GITHUB_CONTENTS_BASE' in LUA_SOURCES
    assert 'api.github.com/repos/skyflarefox/Skyapi/contents' in LUA_SOURCES
    assert 'api.github.com/repos/sushi-dev55-alt/sushitools-games-repo-alt/contents' in LUA_SOURCES
    helper = fn_body(LUA_SOURCES, 'download_github_contents_raw_to_part')
    assert 'application/vnd.github.raw+json' in helper
    assert 'X-GitHub-Api-Version' in helper
    assert 'api.github.com' in helper
    skyflare = fn_body(LUA_SOURCES, 'fetch_skyflare_package')
    sushi = fn_body(LUA_SOURCES, 'fetch_sushi_package')
    assert 'download_github_contents_raw_to_part' in skyflare
    assert 'download_github_contents_raw_to_part' in sushi
    assert 'raw.githubusercontent.com' not in skyflare
    assert 'raw.githubusercontent.com' not in sushi

if __name__ == '__main__':
    tests = [v for k, v in globals().items() if k.startswith('test_') and callable(v)]
    failures = []
    for test in tests:
        try:
            test()
            print('PASS', test.__name__)
        except Exception as exc:
            failures.append((test.__name__, exc))
            print('FAIL', test.__name__, '-', exc)
    if failures:
        raise SystemExit(1)
