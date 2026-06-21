use std::hint::black_box;
use std::time::{SystemTime, UNIX_EPOCH};

const STEAM_KEY_SEED: u8 = 0x5a;
const SGDB_KEY_SEED: u8 = 0x37;
const STEAM_KEY_BYTES: [u8; 32] = [
    0x32, 0x7d, 0x3a, 0x9b, 0xf6, 0xb6, 0x4f, 0xe9, 0x1e, 0xef, 0x8e, 0x1a, 0xb0, 0x9a, 0x4c, 0x65,
    0xd0, 0x67, 0x2e, 0xd8, 0xb6, 0xbe, 0xf0, 0x36, 0xba, 0x7b, 0xa7, 0xfe, 0xe9, 0x83, 0x1d, 0xbd,
];
const SGDB_KEY_BYTES: [u8; 32] = [
    0x02, 0xf5, 0x2b, 0x76, 0xcb, 0xd2, 0x5b, 0xed, 0xfb, 0x4e, 0x8d, 0xff, 0xf9, 0x6a, 0xb3, 0x62,
    0xb9, 0x17, 0x23, 0x12, 0x84, 0x58, 0xcc, 0x38, 0xe5, 0xc9, 0x2e, 0xfb, 0x27, 0x4f, 0x74, 0xaf,
];

pub(super) fn derive_asset_pack_key(salt: &[u8; 16]) -> [u8; 32] {
    let mut steam = decode_obfuscated_key(&STEAM_KEY_BYTES, STEAM_KEY_SEED);
    let mut sgdb = decode_obfuscated_key(&SGDB_KEY_BYTES, SGDB_KEY_SEED);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"0xo asset pack key v1");
    hasher.update(salt);
    hasher.update(&steam);
    hasher.update(&sgdb);
    steam.fill(0);
    sgdb.fill(0);
    *hasher.finalize().as_bytes()
}

#[inline(never)]
pub(super) fn with_steam_api_key<T>(f: impl FnOnce(&str) -> T) -> T {
    let mut key = decode_obfuscated_key(&STEAM_KEY_BYTES, STEAM_KEY_SEED);
    let result = {
        let key = std::str::from_utf8(&key).unwrap_or_default();
        f(key)
    };
    key.fill(0);
    result
}

#[inline(never)]
pub(super) fn with_steamgriddb_key<T>(f: impl FnOnce(&str) -> T) -> T {
    let mut key = decode_obfuscated_key(&SGDB_KEY_BYTES, SGDB_KEY_SEED);
    let result = {
        let key = std::str::from_utf8(&key).unwrap_or_default();
        f(key)
    };
    key.fill(0);
    result
}

#[inline(never)]
fn decode_obfuscated_key(bytes: &[u8], seed: u8) -> Vec<u8> {
    let seed = black_box(seed);
    let mask = runtime_decode_mask();
    black_box(bytes)
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            let rotate = (index as u32 % 7) + 1;
            let mixed = byte.rotate_right(rotate) ^ mask.rotate_left((index as u32 % 5) + 1);
            let unmasked = black_box(mixed) ^ mask.rotate_left((index as u32 % 5) + 1);
            unmasked ^ seed.wrapping_add((index as u8).wrapping_mul(13))
        })
        .collect()
}

#[inline(never)]
fn runtime_decode_mask() -> u8 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as u8)
        .unwrap_or(0);
    black_box(nanos ^ (std::process::id() as u8))
}
