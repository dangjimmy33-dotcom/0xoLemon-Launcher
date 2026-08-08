// SteamTools binary-patch signatures and the scan/resolve engine.
// Patterns, masks, offsets, validators and patch-site resolvers are exact; do not alter.

use crate::pe::PeSection;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScanRegion {
    Text,
    Obfuscated,
    All,
}

/// Resolved patch: where to write, the expected original bytes, the replacement.
#[derive(Clone)]
pub struct PatchEntry {
    pub offset: i64,
    pub original: Vec<u8>,
    pub replacement: Vec<u8>,
}

type Validator = fn(&[u8], usize) -> bool;
type PatchSiteResolver = fn(&[u8], usize) -> i64;

/// Declarative patch definition: pattern + mask to locate the site, plus optional
/// validator / patch-site resolver and relative-scan window.
pub struct PatternPatch {
    pub name: &'static str,
    pub pattern: &'static [u8],
    pub mask: &'static [u8],
    pub patch_offset: i64,
    pub original: &'static [u8],
    pub replacement: &'static [u8],
    pub region: ScanRegion,
    pub wildcard_start: usize,
    pub wildcard_len: usize,
    pub validator: Option<Validator>,
    pub patch_site_resolver: Option<PatchSiteResolver>,
    pub relative_to_patch_index: Option<usize>,
    pub relative_start: i64,
    pub relative_end: i64,
}

impl PatternPatch {
    const fn new(
        name: &'static str,
        pattern: &'static [u8],
        mask: &'static [u8],
        patch_offset: i64,
        original: &'static [u8],
        replacement: &'static [u8],
        region: ScanRegion,
    ) -> Self {
        PatternPatch {
            name,
            pattern,
            mask,
            patch_offset,
            original,
            replacement,
            region,
            wildcard_start: 0,
            wildcard_len: 0,
            validator: None,
            patch_site_resolver: None,
            relative_to_patch_index: None,
            relative_start: 0,
            relative_end: 0,
        }
    }
}

#[inline]
fn read_i32(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// First offset in [start, end) where `pattern` matches under `mask` (0 = wildcard).
pub fn scan_for_pattern(data: &[u8], start: i64, end: i64, pattern: &[u8], mask: &[u8]) -> i64 {
    let limit = (end.min(data.len() as i64)) - pattern.len() as i64;
    let mut i = start;
    while i <= limit {
        let base = i as usize;
        let mut matched = true;
        for j in 0..pattern.len() {
            if mask[j] != 0 && data[base + j] != pattern[j] {
                matched = false;
                break;
            }
        }
        if matched {
            return i;
        }
        i += 1;
    }
    -1
}

pub fn scan_for_bytes(data: &[u8], start: i64, end: i64, needle: &[u8]) -> i64 {
    let limit = (end.min(data.len() as i64)) - needle.len() as i64;
    let mut i = start;
    while i <= limit {
        let base = i as usize;
        if &data[base..base + needle.len()] == needle {
            return i;
        }
        i += 1;
    }
    -1
}

/// Resolve a single PatternPatch to a file offset, or -1.
pub fn resolve_pattern_patch(
    data: &[u8],
    patch: &PatternPatch,
    section_start: i64,
    section_end: i64,
    resolved_offsets: Option<&[i64]>,
) -> i64 {
    let mut scan_start = section_start;
    let mut scan_end = section_end;

    if let (Some(idx), Some(offsets)) = (patch.relative_to_patch_index, resolved_offsets) {
        if idx < offsets.len() && offsets[idx] >= 0 {
            scan_start = section_start.max(offsets[idx] + patch.relative_start);
            scan_end = (offsets[idx] + patch.relative_end).min(section_end);
        }
    }

    let mut pos = scan_start;
    while pos < scan_end {
        let hit = scan_for_pattern(data, pos, scan_end, patch.pattern, patch.mask);
        if hit < 0 {
            break;
        }
        let hit_usize = hit as usize;

        if let Some(validate) = patch.validator {
            if !validate(data, hit_usize) {
                pos = hit + 1;
                continue;
            }
        }

        if let Some(resolve) = patch.patch_site_resolver {
            let resolved = resolve(data, hit_usize);
            if resolved >= 0 {
                return resolved;
            }
            pos = hit + 1;
            continue;
        }

        return hit + patch.patch_offset;
    }
    -1
}

/// Resolve an ordered group of patches; later patches may reference earlier
/// resolved offsets via relative_to_patch_index. Returns None if any required
/// patch fails to resolve.
pub fn resolve_pattern_group(
    data: &[u8],
    patches: &[PatternPatch],
    text_start: i64,
    text_end: i64,
    obf_start: i64,
    obf_end: i64,
    log: &mut dyn FnMut(&str),
) -> Option<Vec<PatchEntry>> {
    let mut result: Vec<PatchEntry> = Vec::with_capacity(patches.len());
    let mut offsets: Vec<i64> = vec![-1; patches.len()];

    for (i, p) in patches.iter().enumerate() {
        let (s_start, s_end) = match p.region {
            ScanRegion::Text => (text_start, text_end),
            ScanRegion::Obfuscated => (obf_start, obf_end),
            ScanRegion::All => (0, data.len() as i64),
        };

        let offset = resolve_pattern_patch(data, p, s_start, s_end, Some(&offsets));
        if offset < 0 {
            log(&format!("  Could not locate {}", p.name));
            return None;
        }
        offsets[i] = offset;
        result.push(snapshot_patch(data, offset, p));
        log(&format!("  {} at 0x{:X}", p.name, offset));
    }
    Some(result)
}

/// Build a PatchEntry by cloning the template original/replacement, then snapshot
/// any wildcard bytes from the actual data at the resolved offset.
fn snapshot_patch(data: &[u8], offset: i64, template: &PatternPatch) -> PatchEntry {
    let mut orig = template.original.to_vec();
    let mut repl = template.replacement.to_vec();
    let len = orig.len();

    if template.wildcard_len > 0
        && template.wildcard_start + template.wildcard_len <= len
        && offset as usize + template.wildcard_start + template.wildcard_len <= data.len()
    {
        let src = offset as usize + template.wildcard_start;
        let slice = &data[src..src + template.wildcard_len];
        orig[template.wildcard_start..template.wildcard_start + template.wildcard_len]
            .copy_from_slice(slice);
        repl[template.wildcard_start..template.wildcard_start + template.wildcard_len]
            .copy_from_slice(slice);
    }

    PatchEntry {
        offset,
        original: orig,
        replacement: repl,
    }
}

// Validators / resolvers (transcribed from the C# closures)

fn core1_validator(data: &[u8], hit: usize) -> bool {
    let opcode = data[hit + 9];
    if opcode == 0xE8 {
        read_i32(data, hit + 10) < 0
    } else {
        opcode == 0xB8
    }
}

fn core2_validator(data: &[u8], hit: usize) -> bool {
    let b = data[hit + 14];
    b == 0x74 || b == 0xEB
}

fn p1_validator(data: &[u8], hit: usize) -> bool {
    (data[hit + 18] == 0x0F && data[hit + 19] == 0x84)
        || (data[hit + 18] == 0x90 && data[hit + 19] == 0xE9)
}

fn p2_validator(data: &[u8], hit: usize) -> bool {
    (data[hit + 22] == 0x8B && data[hit + 23] == 0x0D)
        || (data[hit + 22] == 0x31 && data[hit + 23] == 0xC9)
}

fn p3_resolver(data: &[u8], hit: usize) -> i64 {
    let search_start = hit + 7;
    let search_end = (search_start + 30).min(data.len());
    let mut i = search_start;
    while i + 5 < search_end {
        if data[i] == 0x89 && data[i + 1] == 0x3D {
            return i as i64;
        }
        if data[i] == 0x90
            && data[i + 1] == 0x90
            && data[i + 2] == 0x90
            && data[i + 3] == 0x90
            && data[i + 4] == 0x90
            && data[i + 5] == 0x90
        {
            return i as i64;
        }
        i += 1;
    }
    -1
}

fn p1_validator_v2(data: &[u8], hit: usize) -> bool {
    (data[hit + 15] == 0x0F && data[hit + 16] == 0x86)
        || (data[hit + 15] == 0x90 && data[hit + 16] == 0xE9)
}

fn p2_validator_v2(data: &[u8], hit: usize) -> bool {
    (data[hit + 19] == 0x8B && data[hit + 20] == 0x0D)
        || (data[hit + 19] == 0x31 && data[hit + 20] == 0xC9)
}

fn p4_validator_v2(data: &[u8], hit: usize) -> bool {
    (data[hit + 2] == 0x0F && data[hit + 3] == 0x95 && data[hit + 4] == 0xC0)
        || (data[hit + 2] == 0xB0 && data[hit + 3] == 0x01 && data[hit + 4] == 0x90)
}

fn p7_validator_v2(data: &[u8], hit: usize) -> bool {
    (data[hit + 3] == 0x75 && data[hit + 8] == 0x74)
        || (data[hit + 3] == 0x90 && data[hit + 8] == 0xEB)
}

fn p3_resolver_v2(data: &[u8], hit: usize) -> i64 {
    let search_start = hit + 8;
    let search_end = (search_start + 20).min(data.len());
    let mut i = search_start;
    while i + 5 < search_end {
        if data[i] == 0x89 && data[i + 1] == 0x3D {
            return i as i64;
        }
        if data[i] == 0x90
            && data[i + 1] == 0x90
            && data[i + 2] == 0x90
            && data[i + 3] == 0x90
            && data[i + 4] == 0x90
            && data[i + 5] == 0x90
        {
            return i as i64;
        }
        i += 1;
    }
    -1
}

fn has_bytes(bytes: &[u8], pos: i64, expected: &[u8]) -> bool {
    if pos < 0 || pos as usize + expected.len() > bytes.len() {
        return false;
    }
    &bytes[pos as usize..pos as usize + expected.len()] == expected
}

fn skip_optional_bridge(bytes: &[u8], pos: i64) -> i64 {
    if has_bytes(bytes, pos, &[0xE9]) {
        pos + 5
    } else {
        pos
    }
}

fn p4_resolver_v1(data: &[u8], hit: usize) -> i64 {
    let mut pos = hit as i64 + 3;
    pos = skip_optional_bridge(data, pos);
    if !has_bytes(data, pos, &[0x0F, 0x84]) {
        return -1;
    }
    pos += 6;

    if !has_bytes(data, pos, &[0xE8]) {
        return -1;
    }
    pos += 5;

    if !has_bytes(data, pos, &[0x85, 0xC0]) {
        return -1;
    }
    pos += 2;

    pos = skip_optional_bridge(data, pos);
    if !has_bytes(data, pos, &[0x0F, 0x85]) {
        return -1;
    }
    pos += 6;

    if !has_bytes(data, pos, &[0xC6, 0x05]) {
        return -1;
    }
    if pos as usize + 6 >= data.len() || data[pos as usize + 6] != 0x01 {
        return -1;
    }
    pos += 7;

    pos = skip_optional_bridge(data, pos);
    if !has_bytes(data, pos, &[0xE9]) {
        return -1;
    }
    pos += 5;

    if !has_bytes(data, pos, &[0xC6, 0x05]) {
        return -1;
    }
    if pos as usize + 6 >= data.len() {
        return -1;
    }
    let val = data[pos as usize + 6];
    if val == 0x00 || val == 0x01 {
        pos
    } else {
        -1
    }
}

fn p5_validator(data: &[u8], hit: usize) -> bool {
    if hit + 24 > data.len() {
        return false;
    }
    let opcode = data[hit + 22];
    if opcode != 0x75 && opcode != 0xEB {
        return false;
    }
    let skip_dist = data[hit + 23] as i8 as i64;
    if skip_dist <= 0 {
        return false;
    }
    let after_skip = hit as i64 + 24 + skip_dist;
    if after_skip > data.len() as i64 {
        return false;
    }
    let mut j = hit + 24;
    while (j as i64) < after_skip && j < data.len() - 4 {
        if data[j] == 0xE9 {
            let rel = read_i32(data, j + 1);
            if rel < 0 {
                return true;
            }
        }
        j += 1;
    }
    false
}

fn p6_validator(data: &[u8], hit: usize) -> bool {
    if hit + 48 > data.len() {
        return false;
    }
    let is_old = data[hit + 18] == 0x35
        && data[hit + 19] == 0x36
        && data[hit + 44] == 0x5E
        && data[hit + 45] == 0x00;
    let is_new = data[hit + 18] == 0x35
        && data[hit + 19] == 0x37
        && data[hit + 46] == 0x43
        && data[hit + 47] == 0x00;
    is_old || is_new
}

// Core DLL patches (xinput1_4.dll / dwmapi.dll)

pub fn core_patch_defs() -> Vec<PatternPatch> {
    vec![
        // Core1: NOP download call (E8 -> B8).
        PatternPatch {
            wildcard_start: 1,
            wildcard_len: 4,
            validator: Some(core1_validator),
            ..PatternPatch::new(
                "Core1 (download call)",
                &[
                    0x48, 0x8B, 0x4C, 0x24, 0x00, 0x48, 0x8D, 0x55, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x85, 0xC0, 0x0F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x41, 0x83, 0xFC, 0x01,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF,
                ],
                9,
                &[0xE8, 0x7C, 0xF5, 0xFF, 0xFF],
                &[0xB8, 0x01, 0x00, 0x00, 0x00],
                ScanRegion::Text,
            )
        },
        // Core2: jz -> jmp (hash check bypass), relative to Core1.
        PatternPatch {
            relative_to_patch_index: Some(0),
            relative_start: -0x300,
            relative_end: 0x300,
            validator: Some(core2_validator),
            ..PatternPatch::new(
                "Core2 (hash check jz)",
                &[
                    0x49, 0x8B, 0xD5, 0x48, 0x8D, 0x4D, 0x00, 0xE8, 0x00, 0x00, 0x00, 0x00, 0x85,
                    0xC0, 0x00, 0x00, 0x33, 0xFF, 0xE9,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF,
                    0xFF, 0x00, 0x00, 0xFF, 0xFF, 0xFF,
                ],
                14,
                &[0x74],
                &[0xEB],
                ScanRegion::Text,
            )
        },
    ]
}

// Payload patches P1-P3 (cloud redirect disable)

pub fn payload_p123_defs() -> Vec<PatternPatch> {
    vec![
        // P1: cloud rewrite jz -> nop jmp.
        PatternPatch {
            wildcard_start: 2,
            wildcard_len: 4,
            validator: Some(p1_validator),
            ..PatternPatch::new(
                "P1 (cloud rewrite skip)",
                &[
                    0x44, 0x8B, 0x3D, 0x00, 0x00, 0x00, 0x00, 0x85, 0xC0, 0x0F, 0x85, 0x00, 0x00,
                    0x00, 0x00, 0x45, 0x85, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00,
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF,
                ],
                18,
                &[0x0F, 0x84, 0x3B, 0x01, 0x00, 0x00],
                &[0x90, 0xE9, 0x3B, 0x01, 0x00, 0x00],
                ScanRegion::Text,
            )
        },
        // P2: zero proxy appid load, relative to P1.
        PatternPatch {
            relative_to_patch_index: Some(0),
            relative_start: 0,
            relative_end: 0x500,
            validator: Some(p2_validator),
            ..PatternPatch::new(
                "P2 (proxy appid zero)",
                &[
                    0x48, 0x8B, 0xF0, 0x4C, 0x8B, 0xC7, 0x4C, 0x8B, 0x7C, 0x24, 0x00, 0x49, 0x8B,
                    0xD7, 0x48, 0x8B, 0xC8, 0xE8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x48, 0x8D, 0x14, 0x3E, 0x48, 0x81, 0xF9, 0x80, 0x00, 0x00, 0x00,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0xFF, 0xFF,
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                ],
                22,
                &[0x8B, 0x0D, 0x7D, 0xCA, 0x1B, 0x00],
                &[0x31, 0xC9, 0x90, 0x90, 0x90, 0x90],
                ScanRegion::Text,
            )
        },
        // P3: NOP IPC appid preserve.
        PatternPatch {
            wildcard_start: 2,
            wildcard_len: 4,
            patch_site_resolver: Some(p3_resolver),
            ..PatternPatch::new(
                "P3 (IPC appid preserve)",
                &[0xC7, 0x40, 0x09, 0xE0, 0x01, 0x00, 0x00],
                &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
                0,
                &[0x89, 0x3D, 0x00, 0x00, 0x00, 0x00],
                &[0x90, 0x90, 0x90, 0x90, 0x90, 0x90],
                ScanRegion::Obfuscated,
            )
        },
    ]
}

/// V2 P1/P2/P3: new plain .text payload (no obfuscator, restructured code).
pub fn payload_p123_defs_v2() -> Vec<PatternPatch> {
    vec![
        // P1 V2: cloud rewrite jbe -> nop jmp.
        PatternPatch {
            wildcard_start: 2,
            wildcard_len: 4,
            validator: Some(p1_validator_v2),
            ..PatternPatch::new(
                "P1 (cloud rewrite skip)",
                &[
                    0x85, 0xC0, 0x0F, 0x85, 0x00, 0x00, 0x00, 0x00,
                    0x44, 0x39, 0x25, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,
                    0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00,
                ],
                15,
                &[0x0F, 0x86, 0x00, 0x00, 0x00, 0x00],
                &[0x90, 0xE9, 0x00, 0x00, 0x00, 0x00],
                ScanRegion::Text,
            )
        },
        // P2 V2: zero proxy appid load. Registers changed:
        // mov r15,rax / mov r8,rsi (was mov rsi,rax / mov r8,rdi).
        // lea rdx,[r15+rdi] (was lea rdx,[rsi+rdi]).
        PatternPatch {
            wildcard_start: 2,
            wildcard_len: 4,
            validator: Some(p2_validator_v2),
            ..PatternPatch::new(
                "P2 (proxy appid zero)",
                &[
                    0x4C, 0x8B, 0xF8, 0x4C, 0x8B, 0xC6, 0x48, 0x8B, 0x54, 0x24, 0x30,
                    0x48, 0x8B, 0xC8, 0xE8, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x49, 0x8D, 0x14, 0x37, 0x48, 0x81, 0xF9, 0x80, 0x00, 0x00, 0x00,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                    0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                ],
                19,
                &[0x8B, 0x0D, 0x00, 0x00, 0x00, 0x00],
                &[0x31, 0xC9, 0x90, 0x90, 0x90, 0x90],
                ScanRegion::Text,
            )
        },
        // P3 V2: NOP IPC appid preserve. Anchor changed from C7 40 09
        // (mov [rax+9],480) to 41 C7 46 09 (mov [r14+9],480). Now in .text.
        PatternPatch {
            wildcard_start: 2,
            wildcard_len: 4,
            patch_site_resolver: Some(p3_resolver_v2),
            ..PatternPatch::new(
                "P3 (IPC appid preserve)",
                &[0x41, 0xC7, 0x46, 0x09, 0xE0, 0x01, 0x00, 0x00],
                &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
                0,
                &[0x89, 0x3D, 0x00, 0x00, 0x00, 0x00],
                &[0x90, 0x90, 0x90, 0x90, 0x90, 0x90],
                ScanRegion::Text,
            )
        },
    ]
}

// Payload setup patches P4/P5/P6

/// V1 defs: old payload with obfuscated P4 (E9 bridges).
pub fn payload_setup_defs_v1() -> Vec<PatternPatch> {
    vec![
        PatternPatch {
            wildcard_start: 2,
            wildcard_len: 4,
            patch_site_resolver: Some(p4_resolver_v1),
            ..PatternPatch::new(
                "P4 (activation flag)",
                &[0x4D, 0x85, 0xC0],
                &[0xFF, 0xFF, 0xFF],
                0,
                &[0xC6, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00],
                &[0xC6, 0x05, 0x00, 0x00, 0x00, 0x00, 0x01],
                ScanRegion::Obfuscated,
            )
        },
        // P7 exists in both old and new payloads.
        PatternPatch {
            validator: Some(p7_validator_v2),
            ..PatternPatch::new(
                "P7 (activation confirmed)",
                &[
                    0x4C, 0x3B, 0xC0, 0x75, 0x17, 0x4D, 0x85, 0xC0,
                    0x74, 0x09, 0xE8, 0x00, 0x00, 0x00, 0x00, 0x85,
                    0xC0, 0x75, 0x09, 0xC6, 0x05,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                    0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF,
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                ],
                3,
                &[0x75, 0x17, 0x4D, 0x85, 0xC0, 0x74, 0x09],
                &[0x90, 0x90, 0x4D, 0x85, 0xC0, 0xEB, 0x09],
                ScanRegion::Text,
            )
        },
        PatternPatch {
            validator: Some(p5_validator),
            ..PatternPatch::new(
                "P5 (GetCookie retry skip)",
                &[
                    0x66, 0x48, 0x0F, 0x7E, 0xC7, 0x66, 0x48, 0x0F, 0x7E, 0xCE, 0x48, 0x8D, 0x4D,
                    0x00, 0xE8, 0x00, 0x00, 0x00, 0x00, 0x48, 0x85, 0xF6, 0x00,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                    0x00, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00,
                ],
                22,
                &[0x75],
                &[0xEB],
                ScanRegion::Text,
            )
        },
        PatternPatch {
            validator: Some(p6_validator),
            ..PatternPatch::new(
                "P6 (GMRC pattern fix)",
                &[
                    0x34, 0x38, 0x20, 0x38, 0x39, 0x20, 0x35, 0x43, 0x20, 0x32, 0x34, 0x20, 0x31,
                    0x38, 0x20, 0x35, 0x35, 0x20,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                ],
                0,
                &[
                    0x34, 0x38, 0x20, 0x38, 0x39, 0x20, 0x35, 0x43, 0x20, 0x32, 0x34, 0x20, 0x31,
                    0x38, 0x20, 0x35, 0x35, 0x20, 0x35, 0x36, 0x20, 0x35, 0x37, 0x20, 0x34, 0x31,
                    0x20, 0x35, 0x35, 0x20, 0x34, 0x31, 0x20, 0x35, 0x37, 0x20, 0x34, 0x38, 0x20,
                    0x38, 0x44, 0x20, 0x36, 0x43, 0x5E, 0x00, 0x00, 0x00,
                ],
                &[
                    0x34, 0x38, 0x20, 0x38, 0x39, 0x20, 0x35, 0x43, 0x20, 0x32, 0x34, 0x20, 0x31,
                    0x38, 0x20, 0x35, 0x35, 0x20, 0x35, 0x37, 0x20, 0x34, 0x31, 0x20, 0x35, 0x34,
                    0x20, 0x34, 0x31, 0x20, 0x35, 0x36, 0x20, 0x34, 0x31, 0x20, 0x35, 0x37, 0x20,
                    0x34, 0x38, 0x20, 0x38, 0x44, 0x20, 0x36, 0x43, 0x00,
                ],
                ScanRegion::All,
            )
        },
    ]
}

/// Current defs: new payload with plain .text P4 (no obfuscator).
pub fn payload_setup_defs() -> Vec<PatternPatch> {
    vec![
        // P4: force activation flag to 1.
        // New plain .text layout: test edx,edx; setnz al -> mov al,1; nop.
        PatternPatch {
            validator: Some(p4_validator_v2),
            ..PatternPatch::new(
                "P4 (activation flag)",
                &[
                    0x85, 0xD2, 0x00, 0x00, 0x00, 0xEB, 0x02, 0xB0, 0x01, 0x88,
                    0x47, 0x01,
                ],
                &[
                    0xFF, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                    0xFF, 0xFF,
                ],
                2,
                &[0x0F, 0x95, 0xC0],
                &[0xB0, 0x01, 0x90],
                ScanRegion::Text,
            )
        },
        // P7: force activation confirmed flag to 1.
        // Bypass server-side hash check: jnz->nop+nop, jz->jmp.
        PatternPatch {
            validator: Some(p7_validator_v2),
            ..PatternPatch::new(
                "P7 (activation confirmed)",
                &[
                    0x4C, 0x3B, 0xC0, 0x75, 0x17, 0x4D, 0x85, 0xC0,
                    0x74, 0x09, 0xE8, 0x00, 0x00, 0x00, 0x00, 0x85,
                    0xC0, 0x75, 0x09, 0xC6, 0x05,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                    0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF,
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                ],
                3,
                &[0x75, 0x17, 0x4D, 0x85, 0xC0, 0x74, 0x09],
                &[0x90, 0x90, 0x4D, 0x85, 0xC0, 0xEB, 0x09],
                ScanRegion::Text,
            )
        },
        // P5: skip GetCookie retry.
        PatternPatch {
            validator: Some(p5_validator),
            ..PatternPatch::new(
                "P5 (GetCookie retry skip)",
                &[
                    0x66, 0x48, 0x0F, 0x7E, 0xC7, 0x66, 0x48, 0x0F, 0x7E, 0xCE, 0x48, 0x8D, 0x4D,
                    0x00, 0xE8, 0x00, 0x00, 0x00, 0x00, 0x48, 0x85, 0xF6, 0x00,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                    0x00, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00,
                ],
                22,
                &[0x75],
                &[0xEB],
                ScanRegion::Text,
            )
        },
        // P6: fix GMRC pattern string (May 27 2026 Steam update).
        PatternPatch {
            validator: Some(p6_validator),
            ..PatternPatch::new(
                "P6 (GMRC pattern fix)",
                &[
                    0x34, 0x38, 0x20, 0x38, 0x39, 0x20, 0x35, 0x43, 0x20, 0x32, 0x34, 0x20, 0x31,
                    0x38, 0x20, 0x35, 0x35, 0x20,
                ],
                &[
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                ],
                0,
                &[
                    0x34, 0x38, 0x20, 0x38, 0x39, 0x20, 0x35, 0x43, 0x20, 0x32, 0x34, 0x20, 0x31,
                    0x38, 0x20, 0x35, 0x35, 0x20, 0x35, 0x36, 0x20, 0x35, 0x37, 0x20, 0x34, 0x31,
                    0x20, 0x35, 0x35, 0x20, 0x34, 0x31, 0x20, 0x35, 0x37, 0x20, 0x34, 0x38, 0x20,
                    0x38, 0x44, 0x20, 0x36, 0x43, 0x5E, 0x00, 0x00, 0x00,
                ],
                &[
                    0x34, 0x38, 0x20, 0x38, 0x39, 0x20, 0x35, 0x43, 0x20, 0x32, 0x34, 0x20, 0x31,
                    0x38, 0x20, 0x35, 0x35, 0x20, 0x35, 0x37, 0x20, 0x34, 0x31, 0x20, 0x35, 0x34,
                    0x20, 0x34, 0x31, 0x20, 0x35, 0x36, 0x20, 0x34, 0x31, 0x20, 0x35, 0x37, 0x20,
                    0x34, 0x38, 0x20, 0x38, 0x44, 0x20, 0x36, 0x43, 0x00,
                ],
                ScanRegion::All,
            )
        },
    ]
}

// CloudRedirect hook finders

/// Locate SendPkt via its alloca_probe setup and walk back to the prologue. -1 if absent.
pub fn find_send_pkt_function(data: &[u8], text_start: i64, text_end: i64) -> i64 {
    let needle = [0xB8, 0x00, 0x11, 0x00, 0x00, 0xE8];
    let mut pos = text_start;
    while pos < text_end {
        let hit = scan_for_bytes(data, pos, text_end, &needle);
        if hit < 0 {
            break;
        }
        let func_start = hit - 0x18;
        if func_start < text_start {
            pos = hit + 1;
            continue;
        }
        let fs = func_start as usize;
        if (data[fs] == 0x48
            && data[fs + 1] == 0x89
            && data[fs + 2] == 0x5C
            && data[fs + 3] == 0x24
            && data[fs + 4] == 0x20)
            || data[fs] == 0xE9
        {
            return func_start;
        }
        pos = hit + 1;
    }
    -1
}

/// Find a zero-filled code cave at the tail of an executable PE section, or -1.
pub fn find_code_cave(data: &[u8], sections: &[PeSection], required_size: i64) -> i64 {
    for sec in sections {
        if !sec.is_executable() {
            continue;
        }
        let mut raw_end = (sec.raw_offset + sec.raw_size) as i64;
        if raw_end > data.len() as i64 {
            raw_end = data.len() as i64;
        }
        let raw_start = sec.raw_offset as i64;

        let mut zero_run: i64 = 0;
        let mut i = raw_end - 1;
        while i >= raw_start {
            if data[i as usize] == 0 {
                zero_run += 1;
            } else {
                break;
            }
            i -= 1;
        }
        if zero_run >= required_size {
            return raw_end - zero_run;
        }
    }
    -1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::PeSection;

    // Resolve the Core DLL patches against the live SteamTools core DLL.
    #[test]
    fn resolve_core_patches_live() {
        let candidates = [
            r"C:\Games\Steam\xinput1_4.dll",
            r"C:\Games\Steam\dwmapi.dll",
            r"C:\Program Files (x86)\Steam\xinput1_4.dll",
            r"C:\Program Files (x86)\Steam\dwmapi.dll",
        ];
        let path = candidates.iter().find(|p| std::path::Path::new(p).is_file());
        let Some(path) = path else {
            eprintln!("No SteamTools core DLL found; skipping");
            return;
        };
        let dll = std::fs::read(path).expect("read dll");
        let sections = PeSection::parse(&dll);
        let text = match PeSection::find(&sections, ".text") {
            Some(t) => t,
            None => {
                eprintln!("{}: no .text; skipping (not a SteamTools core DLL?)", path);
                return;
            }
        };
        let t_start = text.raw_offset as i64;
        let t_end = (t_start + text.raw_size as i64).min(dll.len() as i64);

        let mut log = |m: &str| eprintln!("{}", m);
        let defs = core_patch_defs();
        let result = resolve_pattern_group(&dll, &defs, t_start, t_end, 0, 0, &mut log);
        match result {
            Some(entries) => {
                eprintln!("Core patches resolved in {}:", path);
                for e in &entries {
                    eprintln!(
                        "  @0x{:X}  orig={:02X?} repl={:02X?}",
                        e.offset, e.original, e.replacement
                    );
                }
            }
            None => eprintln!(
                "Core patches did NOT resolve in {} (may be a different SteamTools version)",
                path
            ),
        }
    }
}

/// Locate recvPktGlobal: find `lea rcx, SendPkt`, then the nearby
/// `mov [rip+disp], rcx` that stores the original function pointer.
/// Backward scan first, forward scan as fallback.
pub fn find_recv_pkt_global_rva(
    data: &[u8],
    sections: &[PeSection],
    send_pkt_rva: i64,
    search_start: i64,
    search_end: i64,
) -> i64 {
    let mut i = search_start;
    while i < search_end - 7 {
        let iu = i as usize;
        if data[iu] != 0x48 || data[iu + 1] != 0x8D || data[iu + 2] != 0x0D {
            i += 1;
            continue;
        }
        let rel = read_i32(data, iu + 3) as i64;
        let instr_rva = PeSection::file_offset_to_rva(sections, i);
        if instr_rva < 0 {
            i += 1;
            continue;
        }
        let target_rva = instr_rva + 7 + rel;
        if target_rva != send_pkt_rva {
            i += 1;
            continue;
        }
        // Backward-scan for `mov [rip+disp], rcx` (48 89 0D).
        let bstart = search_start.max(i - 0x100);
        let mut j = i - 1;
        while j >= bstart {
            let ju = j as usize;
            if data[ju] == 0x48 && data[ju + 1] == 0x89 && data[ju + 2] == 0x0D {
                let mov_rel = read_i32(data, ju + 3) as i64;
                let mov_rva = PeSection::file_offset_to_rva(sections, j);
                if mov_rva >= 0 {
                    return mov_rva + 7 + mov_rel;
                }
            }
            j -= 1;
        }
        // Forward-scan fallback (48 89 xx, modrm rip-relative).
        let mut j = i + 7;
        let jend = (i + 0x100).min(search_end) - 7;
        while j < jend {
            let ju = j as usize;
            if data[ju] == 0x48 && data[ju + 1] == 0x89 && (data[ju + 2] & 0xC7) == 0x05 {
                let mov_rel = read_i32(data, ju + 3) as i64;
                let mov_rva = PeSection::file_offset_to_rva(sections, j);
                if mov_rva >= 0 {
                    return mov_rva + 7 + mov_rel;
                }
            }
            j += 1;
        }
        i += 1;
    }
    -1
}
