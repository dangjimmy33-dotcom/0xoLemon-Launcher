const crypto = require('crypto');
const { parse } = require('node-html-parser');
const { LuaShopError } = require('./quota');

const CATALOG_URL = 'https://api.steampowered.com/IStoreService/GetAppList/v1/';
const STORE_SEARCH_URL = 'https://store.steampowered.com/search/results/';
const CACHE_TTL_MS = 6 * 60 * 60 * 1000;
const CATALOG_PAGE_SIZE = 50_000;
const CATALOG_MAX_PAGES = 16;
const CATALOG_MAX_ITEMS = 500_000;
const STORE_SEARCH_PAGE_SIZE = 50;
const STORE_SEARCH_MAX_RESPONSE_BYTES = 2 * 1024 * 1024;

let cache = null;
let inFlight = null;

function normalize(value) {
  return String(value || '')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, ' ')
    .trim();
}

function queryHash(query) {
  return crypto.createHash('sha256').update(query).digest('hex').slice(0, 16);
}

function encodeCursor(query, offset) {
  return Buffer.from(JSON.stringify({ q: queryHash(query), o: offset }), 'utf8').toString('base64url');
}

function decodeCursor(query, cursor) {
  if (!cursor) return 0;
  try {
    const parsed = JSON.parse(Buffer.from(cursor, 'base64url').toString('utf8'));
    if (parsed.q !== queryHash(query) || !Number.isSafeInteger(parsed.o) || parsed.o < 0) {
      throw new Error('invalid');
    }
    return parsed.o;
  } catch {
    throw new LuaShopError('INVALID_CURSOR', 'Catalog cursor is invalid.');
  }
}

function catalogEntry(entry) {
  const appid = Number(entry && (entry.appid ?? entry.id));
  const name = String(entry && entry.name || '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 256);
  if (!Number.isSafeInteger(appid) || appid <= 0 || !name) return null;
  const rawHeaderImage = String(entry && entry.headerImage || '').trim();
  const headerImage = /^https:\/\//i.test(rawHeaderImage) ? rawHeaderImage.slice(0, 1024) : '';
  return { appid, name, normalized: normalize(name), headerImage };
}

function catalogMatchScore(entry, normalizedQuery) {
  if (!normalizedQuery) return 0;
  if (/^\d+$/.test(normalizedQuery)) {
    return String(entry.appid) === normalizedQuery ? 0 : null;
  }

  const queryTokens = normalizedQuery.split(' ').filter(Boolean);
  const nameTokens = entry.normalized.split(' ').filter(Boolean);
  if (queryTokens.length === 0 || queryTokens.length > nameTokens.length) return null;
  if (entry.normalized === normalizedQuery) return 0;

  let bestScore = Number.POSITIVE_INFINITY;
  for (let start = 0; start <= nameTokens.length - queryTokens.length; start += 1) {
    let exactTokens = 0;
    let matches = true;
    for (let index = 0; index < queryTokens.length; index += 1) {
      const queryToken = queryTokens[index];
      const nameToken = nameTokens[start + index];
      if (!nameToken.startsWith(queryToken)) {
        matches = false;
        break;
      }
      if (nameToken === queryToken) exactTokens += 1;
    }
    if (!matches) continue;

    // Consecutive word-prefix matching keeps typeahead useful while excluding
    // Steam Store's unrelated fuzzy matches. Exact leading phrases rank first.
    const positionPenalty = start === 0 ? 10 : 100 + start;
    const completionPenalty = queryTokens.length - exactTokens;
    bestScore = Math.min(bestScore, positionPenalty + completionPenalty);
  }
  return Number.isFinite(bestScore) ? bestScore : null;
}

function searchCatalogEntries(apps, query) {
  const normalizedQuery = normalize(query);
  if (!normalizedQuery) return apps;
  return apps
    .map((entry) => ({ entry, score: catalogMatchScore(entry, normalizedQuery) }))
    .filter((match) => match.score != null)
    .sort((left, right) =>
      left.score - right.score
      || left.entry.normalized.length - right.entry.normalized.length
      || left.entry.name.localeCompare(right.entry.name, 'en', { sensitivity: 'base' })
    )
    .map((match) => match.entry);
}

async function loadCatalogPages(fetchImpl, apiKey) {
  const byAppId = new Map();
  let lastAppId = 0;

  for (let page = 0; page < CATALOG_MAX_PAGES; page += 1) {
    const url = new URL(CATALOG_URL);
    url.searchParams.set('key', apiKey);
    url.searchParams.set('max_results', String(CATALOG_PAGE_SIZE));
    url.searchParams.set('include_games', 'true');
    url.searchParams.set('include_dlc', 'false');
    url.searchParams.set('include_software', 'false');
    url.searchParams.set('include_videos', 'false');
    url.searchParams.set('include_hardware', 'false');
    if (lastAppId > 0) url.searchParams.set('last_appid', String(lastAppId));

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 30_000);
    let response;
    try {
      response = await fetchImpl(url, {
        headers: { 'User-Agent': '0xoLemon-Backend/2 LuaCatalog' },
        signal: controller.signal
      });
    } finally {
      clearTimeout(timeout);
    }
    if (!response.ok) throw new Error(`STEAM_CATALOG_HTTP_${response.status}`);
    const payload = await response.json();
    const body = payload && payload.response;
    if (!body || !Array.isArray(body.apps)) throw new Error('STEAM_CATALOG_INVALID');

    for (const rawEntry of body.apps) {
      const entry = catalogEntry(rawEntry);
      if (entry) byAppId.set(entry.appid, entry);
    }
    if (byAppId.size > CATALOG_MAX_ITEMS) throw new Error('STEAM_CATALOG_TOO_LARGE');

    const responseLastAppId = Number(body.last_appid);
    const finalEntryAppId = body.apps.length > 0
      ? Number(body.apps[body.apps.length - 1].appid)
      : 0;
    const nextLastAppId = Number.isSafeInteger(responseLastAppId) && responseLastAppId > 0
      ? responseLastAppId
      : finalEntryAppId;
    const hasMore = body.have_more_results === true
      || (body.have_more_results == null && body.apps.length === CATALOG_PAGE_SIZE);
    if (!hasMore) break;
    if (!Number.isSafeInteger(nextLastAppId) || nextLastAppId <= lastAppId) {
      throw new Error('STEAM_CATALOG_CURSOR_STALLED');
    }
    lastAppId = nextLastAppId;
    if (page === CATALOG_MAX_PAGES - 1) throw new Error('STEAM_CATALOG_PAGE_LIMIT');
  }

  return [...byAppId.values()].sort((left, right) =>
    left.name.localeCompare(right.name, 'en', { sensitivity: 'base' })
  );
}

async function fetchCatalog() {
  const now = Date.now();
  if (cache && cache.expiresAt > now) return cache.apps;
  if (inFlight) return inFlight;
  inFlight = (async () => {
    const apiKey = String(process.env.STEAM_WEB_API_KEY || '').trim();
    if (!apiKey) {
      throw new LuaShopError(
        'STEAM_CATALOG_KEY_REQUIRED',
        'Full Steam catalog browsing requires STEAM_WEB_API_KEY on the backend.',
        503
      );
    }
    try {
      const apps = await loadCatalogPages(fetch, apiKey);
      cache = { apps, expiresAt: Date.now() + CACHE_TTL_MS };
      return apps;
    } finally {
      inFlight = null;
    }
  })();
  return inFlight;
}

async function readLimitedText(response, limit) {
  const declaredLength = Number(response.headers && response.headers.get('content-length'));
  if (Number.isSafeInteger(declaredLength) && declaredLength > limit) {
    throw new Error('STEAM_STORE_SEARCH_TOO_LARGE');
  }
  if (!response.body) {
    const text = await response.text();
    if (Buffer.byteLength(text, 'utf8') > limit) throw new Error('STEAM_STORE_SEARCH_TOO_LARGE');
    return text;
  }
  const chunks = [];
  let size = 0;
  for await (const chunk of response.body) {
    const bytes = Buffer.from(chunk);
    size += bytes.length;
    if (size > limit) throw new Error('STEAM_STORE_SEARCH_TOO_LARGE');
    chunks.push(bytes);
  }
  return Buffer.concat(chunks, size).toString('utf8');
}

function parseStoreSearchPayload(payload) {
  if (!payload || payload.success !== 1 || typeof payload.results_html !== 'string') {
    throw new Error('STEAM_STORE_SEARCH_INVALID');
  }
  const total = Number(payload.total_count);
  const start = Number(payload.start);
  if (!Number.isSafeInteger(total) || total < 0 || !Number.isSafeInteger(start) || start < 0) {
    throw new Error('STEAM_STORE_SEARCH_INVALID');
  }
  const root = parse(payload.results_html);
  const slots = root.querySelectorAll('a.search_result_row').map((row) => {
    const itemKey = String(row.getAttribute('data-ds-itemkey') || '');
    const rawAppId = String(row.getAttribute('data-ds-appid') || '');
    if (!itemKey.startsWith('App_') || !/^\d{1,10}$/.test(rawAppId)) return null;
    return catalogEntry({
      appid: Number(rawAppId),
      name: row.querySelector('.title')?.textContent || '',
      headerImage: row.querySelector('.search_capsule img')?.getAttribute('src') || ''
    });
  });
  return { slots, start, total };
}

async function searchPublicStoreStrict(query, offset, limit, fetchImpl = fetch) {
  const normalizedQuery = normalize(query);
  const items = [];
  const seen = new Set();
  let scanOffset = offset;
  let total = null;

  while (items.length < limit && (total == null || scanOffset < total)) {
    const blockStart = Math.floor(scanOffset / STORE_SEARCH_PAGE_SIZE) * STORE_SEARCH_PAGE_SIZE;
    const block = await fetchStoreSearchBlock(query, blockStart, fetchImpl);
    total = block.total;
    let slotIndex = scanOffset - blockStart;
    if (slotIndex >= block.slots.length) {
      scanOffset = blockStart + STORE_SEARCH_PAGE_SIZE;
      if (block.slots.length < STORE_SEARCH_PAGE_SIZE) break;
      continue;
    }
    while (slotIndex < block.slots.length && items.length < limit) {
      const entry = block.slots[slotIndex];
      scanOffset += 1;
      slotIndex += 1;
      if (!entry || seen.has(entry.appid)) continue;
      const score = catalogMatchScore(entry, normalizedQuery);
      if (score == null) continue;
      seen.add(entry.appid);
      items.push({ entry, score });
    }
    if (block.slots.length < STORE_SEARCH_PAGE_SIZE) break;
  }

  items.sort((left, right) =>
    left.score - right.score
    || left.entry.normalized.length - right.entry.normalized.length
    || left.entry.name.localeCompare(right.entry.name, 'en', { sensitivity: 'base' })
  );
  const exhausted = total != null && scanOffset >= total;
  return {
    items: items.map((match) => match.entry),
    nextOffset: exhausted ? null : scanOffset,
    totalEstimate: offset === 0 && exhausted ? items.length : null
  };
}

async function fetchStoreSearchBlock(query, start, fetchImpl) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 12_000);
  const url = new URL(STORE_SEARCH_URL);
  url.searchParams.set('term', query);
  url.searchParams.set('cc', 'us');
  url.searchParams.set('l', 'english');
  url.searchParams.set('start', String(start));
  url.searchParams.set('count', String(STORE_SEARCH_PAGE_SIZE));
  url.searchParams.set('infinite', '1');
  // Lua Shop installs games, not DLC, soundtracks, demos, or upgrade packs.
  url.searchParams.set('category1', '998');
  try {
    const response = await fetchImpl(url, {
      headers: {
        Accept: 'application/json',
        'User-Agent': '0xoLemon-Backend/2 LuaCatalog'
      },
      signal: controller.signal
    });
    if (!response.ok) throw new Error(`STEAM_STORE_SEARCH_HTTP_${response.status}`);
    const text = await readLimitedText(response, STORE_SEARCH_MAX_RESPONSE_BYTES);
    let payload;
    try {
      payload = JSON.parse(text);
    } catch {
      throw new Error('STEAM_STORE_SEARCH_INVALID');
    }
    const block = parseStoreSearchPayload(payload);
    if (block.start !== start) throw new Error('STEAM_STORE_SEARCH_CURSOR_STALLED');
    return block;
  } finally {
    clearTimeout(timeout);
  }
}

async function searchPublicStore(query, offset, limit, fetchImpl = fetch) {
  const items = [];
  const seen = new Set();
  let scanOffset = offset;
  let total = null;

  while (items.length < limit && (total == null || scanOffset < total)) {
    const blockStart = Math.floor(scanOffset / STORE_SEARCH_PAGE_SIZE) * STORE_SEARCH_PAGE_SIZE;
    const block = await fetchStoreSearchBlock(query, blockStart, fetchImpl);
    total = block.total;
    let slotIndex = scanOffset - blockStart;
    if (slotIndex >= block.slots.length) {
      scanOffset = blockStart + STORE_SEARCH_PAGE_SIZE;
      if (block.slots.length < STORE_SEARCH_PAGE_SIZE) break;
      continue;
    }
    while (slotIndex < block.slots.length && items.length < limit) {
      const entry = block.slots[slotIndex];
      scanOffset += 1;
      slotIndex += 1;
      if (entry && !seen.has(entry.appid)) {
        seen.add(entry.appid);
        items.push(entry);
      }
    }
    if (block.slots.length < STORE_SEARCH_PAGE_SIZE) break;
  }

  return {
    items,
    nextOffset: total != null && scanOffset < total ? scanOffset : null,
    total
  };
}

async function searchSteamCatalog({ query = '', cursor = '', limit = 40 }) {
  const text = String(query).trim().slice(0, 120);
  const normalizedQuery = normalize(text);
  const safeLimit = Math.max(1, Math.min(40, Number(limit) || 40));
  const offset = decodeCursor(normalizedQuery, cursor);
  if (normalizedQuery) {
    const result = await searchPublicStoreStrict(text, offset, safeLimit);
    return {
      items: result.items.map((entry) => ({
        appid: entry.appid,
        name: entry.name,
        headerImage: entry.headerImage
      })),
      nextCursor: result.nextOffset == null
        ? null
        : encodeCursor(normalizedQuery, result.nextOffset),
      totalEstimate: result.totalEstimate
    };
  }
  const apps = await fetchCatalog();
  const matches = searchCatalogEntries(apps, text);
  const page = matches.slice(offset, offset + safeLimit);
  return {
    items: page.map((entry) => ({
      appid: entry.appid,
      name: entry.name,
      // Keep the legacy path for already-released clients. New desktop builds
      // treat catalog images as provisional and replace them with Steam's
      // verified hashed URL when the card enters the viewport.
      headerImage: `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${entry.appid}/header.jpg`
    })),
    nextCursor: offset + page.length < matches.length
      ? encodeCursor(normalizedQuery, offset + page.length)
      : null,
    totalEstimate: matches.length
  };
}

function clearCatalogCache() {
  cache = null;
}

module.exports = {
  clearCatalogCache,
  loadCatalogPages,
  normalize,
  parseStoreSearchPayload,
  searchCatalogEntries,
  searchPublicStore,
  searchPublicStoreStrict,
  searchSteamCatalog
};
