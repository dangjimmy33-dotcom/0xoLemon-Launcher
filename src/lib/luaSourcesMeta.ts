import type { LuaSourceProvider } from '../types'

export type LuaSourceType = 'live' | 'locked' | 'hybrid'

export type LuaSourceMeta = {
  provider: LuaSourceProvider
  displayName: string
  sourceType: LuaSourceType
  sourceTypeLabel: string
  stars: number
  rankLabel: string
  summary: string
  details: string
  whenToUse: string
  complementarity: string
}

export const LUA_SOURCES_METADATA: Record<string, LuaSourceMeta> = {
  hubcap: {
    provider: 'hubcap',
    displayName: 'Hubcap',
    sourceType: 'hybrid',
    sourceTypeLabel: 'Live & Locked (Toàn diện)',
    stars: 5,
    rankLabel: 'Nguồn chuẩn #1 Toàn diện',
    summary: 'Nguồn cao cấp chuẩn nhất, kết hợp hoàn hảo giữa cập nhật Live động và kho Manifest / Depot Key lịch sử.',
    details: 'Hubcap có cơ sở dữ liệu lớn và chính xác nhất, cho phép tải Lua gốc sạch tự động theo dõi cập nhật Steam hoặc ghim phiên bản BuildID lịch sử theo ý muốn.',
    whenToUse: 'Luôn là ưu tiên hàng đầu cho mọi game nếu bạn có Hubcap API Key.',
    complementarity: 'Nguồn chuẩn số 1, bổ trợ hoàn hảo cho cả chế độ Live lẫn Version Locked.',
  },
  openLua: {
    provider: 'openLua',
    displayName: 'OpenLua.cloud',
    sourceType: 'live',
    sourceTypeLabel: 'Nguồn Live (Tự cập nhật)',
    stars: 4.5,
    rankLabel: 'Nguồn bổ trợ Live #1',
    summary: 'Nguồn Lua gốc sạch trực tiếp, không ghim version, tự động theo dõi và cập nhật game mới nhất theo Steam.',
    details: 'OpenLua cung cấp cấu hình Lua gốc sạch cho hàng ngàn game. Launcher đã tích hợp quy trình tải có kiểm soát; khi OpenLua yêu cầu xác thực hoặc quảng cáo, cửa sổ chỉ cho phép thao tác ở vùng Cloudflare hoặc nút đóng quảng cáo rồi tiếp tục tự động.',
    whenToUse: 'Lựa chọn số 1 khi muốn chơi phiên bản game mới nhất (Live mode) hoàn toàn miễn phí không cần API key.',
    complementarity: 'Nguồn bổ trợ chuẩn thứ 2 tốt nhất đối với nguồn Live (sau Hubcap).',
  },
  openlua: {
    provider: 'openLua',
    displayName: 'OpenLua.cloud',
    sourceType: 'live',
    sourceTypeLabel: 'Nguồn Live (Tự cập nhật)',
    stars: 4.5,
    rankLabel: 'Nguồn bổ trợ Live #1',
    summary: 'Nguồn Lua gốc sạch trực tiếp, không ghim version, tự động theo dõi và cập nhật game mới nhất theo Steam.',
    details: 'OpenLua cung cấp cấu hình Lua gốc sạch cho hàng ngàn game. Launcher đã tích hợp quy trình tải có kiểm soát; khi OpenLua yêu cầu xác thực hoặc quảng cáo, cửa sổ chỉ cho phép thao tác ở vùng Cloudflare hoặc nút đóng quảng cáo rồi tiếp tục tự động.',
    whenToUse: 'Lựa chọn số 1 khi muốn chơi phiên bản game mới nhất (Live mode) hoàn toàn miễn phí không cần API key.',
    complementarity: 'Nguồn bổ trợ chuẩn thứ 2 tốt nhất đối với nguồn Live (sau Hubcap).',
  },
  huggingFace: {
    provider: 'huggingFace',
    displayName: 'Cache cộng đồng (HuggingFace)',
    sourceType: 'live',
    sourceTypeLabel: 'Nguồn Live / Tuyển chọn',
    stars: 4,
    rankLabel: 'Kho Cache tuyển chọn',
    summary: 'Kho dữ liệu Lua được tuyển chọn sẵn và kiểm định bởi cộng đồng.',
    details: 'Tốc độ tải tức thì, dữ liệu sạch và ổn định cao cho hàng trăm game phổ biến không cần cấu hình phức tạp.',
    whenToUse: 'Dùng khi muốn cài đặt nhanh các tựa game nổi tiếng mà không cần chờ.',
    complementarity: 'Nguồn bổ trợ cực kỳ ổn định cho chế độ Live.',
  },
  sushi: {
    provider: 'sushi',
    displayName: 'Sushi Tools',
    sourceType: 'live',
    sourceTypeLabel: 'Nguồn Live & Cập nhật',
    stars: 4,
    rankLabel: 'Nguồn cộng đồng Sushi',
    summary: 'Kho mã nguồn Lua cập nhật liên tục từ dự án Sushi Tools.',
    details: 'Cung cấp cấu hình Lua mới cho nhiều tựa game ra mắt gần đây, tương thích tốt với game mới và hoàn toàn miễn phí.',
    whenToUse: 'Dùng thay thế hoàn hảo khi Hubcap hết hạn mức hoặc các nguồn khác chưa cập nhật game mới.',
    complementarity: 'Nguồn bổ trợ dự phòng chất lượng cao cho Live mode.',
  },
  githubMirrors: {
    provider: 'githubMirrors',
    displayName: 'GitHub Manifest Mirrors (75k+)',
    sourceType: 'locked',
    sourceTypeLabel: 'Nguồn Locked (Ghim Manifest)',
    stars: 4,
    rankLabel: 'Kho Locked 75k+ Game',
    summary: 'Kho dữ liệu đồ sộ hơn 75.000 game ghim sẵn Depot Manifest cố định.',
    details: 'Được sao lưu từ nhiều mirror GitHub uy tín. Cung cấp manifest ID chính xác để Steam tải đúng depot phiên bản đó.',
    whenToUse: 'Lý tưởng khi bạn muốn giữ phiên bản game ổn định, tránh bị lỗi khi Steam cập nhật bản mới.',
    complementarity: 'Nguồn bổ trợ số 1 cho chế độ Version Locked.',
  },
  gitHubMirrors: {
    provider: 'githubMirrors',
    displayName: 'GitHub Manifest Mirrors (75k+)',
    sourceType: 'locked',
    sourceTypeLabel: 'Nguồn Locked (Ghim Manifest)',
    stars: 4,
    rankLabel: 'Kho Locked 75k+ Game',
    summary: 'Kho dữ liệu đồ sộ hơn 75.000 game ghim sẵn Depot Manifest cố định.',
    details: 'Được sao lưu từ nhiều mirror GitHub uy tín. Cung cấp manifest ID chính xác để Steam tải đúng depot phiên bản đó.',
    whenToUse: 'Lý tưởng khi bạn muốn giữ phiên bản game ổn định, tránh bị lỗi khi Steam cập nhật bản mới.',
    complementarity: 'Nguồn bổ trợ số 1 cho chế độ Version Locked.',
  },
  steamTools: {
    provider: 'steamTools',
    displayName: 'SteamTools.site',
    sourceType: 'locked',
    sourceTypeLabel: 'Nguồn Locked / Backup',
    stars: 3,
    rankLabel: 'Nguồn dự phòng SteamTools',
    summary: 'Nguồn sao lưu dữ liệu cấu hình từ hệ sinh thái SteamTools.',
    details: 'Kho dữ liệu dự phòng chứa các script ghim manifest và cấu hình depot.',
    whenToUse: 'Dùng khi các nguồn chính thiếu dữ liệu cho game cũ.',
    complementarity: 'Nguồn bổ trợ dự phòng cấp 2 cho Locked mode.',
  },
  steamtools: {
    provider: 'steamTools',
    displayName: 'SteamTools.site',
    sourceType: 'locked',
    sourceTypeLabel: 'Nguồn Locked / Backup',
    stars: 3,
    rankLabel: 'Nguồn dự phòng SteamTools',
    summary: 'Nguồn sao lưu dữ liệu cấu hình từ hệ sinh thái SteamTools.',
    details: 'Kho dữ liệu dự phòng chứa các script ghim manifest và cấu hình depot.',
    whenToUse: 'Dùng khi các nguồn chính thiếu dữ liệu cho game cũ.',
    complementarity: 'Nguồn bổ trợ dự phòng cấp 2 cho Locked mode.',
  },
  luie: {
    provider: 'luie',
    displayName: 'Luie',
    sourceType: 'live',
    sourceTypeLabel: 'LIVE · Bare Lua',
    stars: 4.5,
    rankLabel: 'LuaTools dynamic source',
    summary: 'LuaTools dynamic source that returns a standalone Lua payload.',
    details: 'Luie is kept strictly as a Live provider. Its Lua is validated for the requested AppID and stored in its own provider backup namespace; manifests are never borrowed from another source.',
    whenToUse: 'Use when Luie is reported available and you want a Live Lua source.',
    complementarity: 'Independent Live fallback. The launcher checks availability directly and signs in to LuaTools only for downloads.',
  },
  twentyTwoCloud: {
    provider: 'twentyTwoCloud',
    displayName: 'DepotBox',
    sourceType: 'hybrid',
    sourceTypeLabel: 'LIVE & LOCKED · Free Web / Direct API',
    stars: 4.5,
    rankLabel: 'DepotBox hybrid source',
    summary: 'DepotBox supports both standalone Live Lua and Locked Lua + manifest ZIP packages.',
    details: 'Free Web opens the official DepotBox page and continues automatically from the channel already selected in the launcher: Live selects Download .lua and Locked selects Download .zip. It searches the AppID, selects the game, triggers the matching purple action, and after generation triggers the matching final download only if the embedded WebView did not start it automatically. Verification stays manual only when DepotBox explicitly requires it. If you save your own paid API key, Direct API mode runs entirely in the Rust backend and fetches the selected channel automatically. The legacy internal provider id is retained only for state compatibility.',
    whenToUse: 'Use Free Web without a key, or configure an optional DepotBox API key in Settings for Direct API downloads.',
    complementarity: 'Independent hybrid source. Live and Locked payloads stay isolated under DepotBox and never borrow manifests from another provider.',
  },
  skyflare: {
    provider: 'skyflare',
    displayName: 'Skyflare',
    sourceType: 'live',
    sourceTypeLabel: 'LIVE · Skyapi',
    stars: 4,
    rankLabel: 'Skyflare Skyapi source',
    summary: 'Skyflare publishes AppID-named ZIPs containing the provider Lua payload.',
    details: 'The launcher downloads <AppID>.zip directly from skyflarefox/Skyapi, validates the Lua for the requested AppID, keeps Skyflare isolated in its own backup namespace, and installs it strictly as Live. Legacy manifest pins in the Lua are removed only from the active Live representation.',
    whenToUse: 'Use as an independent Live source when the AppID ZIP exists in Skyapi.',
    complementarity: 'Direct GitHub-backed Live fallback. It no longer depends on LuaTools discovery and never borrows manifests from another provider.',
  },
  ryuu: {
    provider: 'ryuu',
    displayName: 'Ryuu',
    sourceType: 'hybrid',
    sourceTypeLabel: 'LIVE & LOCKED · Official API',
    stars: 4.5,
    rankLabel: 'Ryuu official source',
    summary: 'Official Ryuu manifest generator API with authenticated package downloads.',
    details: 'Ryuu returns the provider ZIP through generator.ryuu.lol. The launcher keeps the raw Lua + manifest package for Locked and removes only setManifestid for the Live representation, so the same provider can switch between both channels without mixing sources.',
    whenToUse: 'Use after saving your own Ryuu auth key in Settings.',
    complementarity: 'Official authenticated Live & Locked fallback with an isolated source backup.',
  },
}

export function getLuaSourceMeta(provider: string, copy: any): LuaSourceMeta {
  const key = provider.toLowerCase()
  let baseMeta: any = { sourceType: 'live', stars: 3 }
  let transKey = 'fallback'

  for (const [k, v] of Object.entries(LUA_SOURCES_METADATA)) {
    if (k.toLowerCase() === key) {
      baseMeta = v
      transKey = k === 'openlua' ? 'openLua' : (k === 'gitHubMirrors' ? 'githubMirrors' : k)
      break
    }
  }

  const t = copy?.sourcesMeta?.[transKey] || copy?.sourcesMeta?.fallback || {}

  return {
    provider: provider as LuaSourceProvider,
    sourceType: baseMeta.sourceType,
    stars: baseMeta.stars,
    displayName: (t.displayName || baseMeta.displayName || provider).replace('{provider}', provider),
    sourceTypeLabel: (t.sourceTypeLabel || baseMeta.sourceTypeLabel || 'Nguồn Lua').replace('{provider}', provider),
    rankLabel: (t.rankLabel || baseMeta.rankLabel || 'Nguồn dữ liệu').replace('{provider}', provider),
    summary: (t.summary || baseMeta.summary || `Nguồn Lua ${provider}`).replace('{provider}', provider),
    details: (t.details || baseMeta.details || `Nguồn cung cấp cấu hình Lua ${provider}`).replace('{provider}', provider),
    whenToUse: (t.whenToUse || baseMeta.whenToUse || 'Dùng khi khả dụng.').replace('{provider}', provider),
    complementarity: (t.complementarity || baseMeta.complementarity || 'Nguồn bổ trợ.').replace('{provider}', provider),
  }
}
