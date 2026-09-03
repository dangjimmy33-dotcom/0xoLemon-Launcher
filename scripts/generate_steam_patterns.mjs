import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';

function sha256(buf) {
  return crypto.createHash('sha256').update(buf).digest('hex').toLowerCase();
}

function parseSections(buffer) {
  const e_lfanew = buffer.readUInt32LE(0x3C);
  const peSignature = buffer.readUInt32LE(e_lfanew);
  if (peSignature !== 0x00004550) throw new Error('Invalid PE signature');

  const numberOfSections = buffer.readUInt16LE(e_lfanew + 4 + 2);
  const sizeOfOptionalHeader = buffer.readUInt16LE(e_lfanew + 4 + 16);
  const optionalHeaderOffset = e_lfanew + 4 + 20;
  const sectionHeadersOffset = optionalHeaderOffset + sizeOfOptionalHeader;

  const sections = [];
  for (let i = 0; i < numberOfSections; i++) {
    const offset = sectionHeadersOffset + i * 40;
    const name = buffer.toString('utf8', offset, offset + 8).replace(/\0/g, '');
    const virtualSize = buffer.readUInt32LE(offset + 8);
    const virtualAddress = buffer.readUInt32LE(offset + 12);
    const sizeOfRawData = buffer.readUInt32LE(offset + 16);
    const pointerToRawData = buffer.readUInt32LE(offset + 20);

    sections.push({
      name,
      virtualSize,
      virtualAddress,
      sizeOfRawData,
      pointerToRawData,
    });
  }
  return sections;
}

function parseSig(sigStr) {
  const parts = sigStr.trim().split(/\s+/);
  const pattern = [];
  const mask = [];
  for (const p of parts) {
    if (p === '??' || p === '?') {
      pattern.push(0);
      mask.push(false);
    } else {
      pattern.push(parseInt(p, 16));
      mask.push(true);
    }
  }
  return { pattern: Buffer.from(pattern), mask };
}

function scanPattern(data, start, end, { pattern, mask }) {
  const pLen = pattern.length;
  const limit = Math.min(end, data.length) - pLen;
  for (let i = start; i <= limit; i++) {
    let matched = true;
    for (let j = 0; j < pLen; j++) {
      if (mask[j] && data[i + j] !== pattern[j]) {
        matched = false;
        break;
      }
    }
    if (matched) return i;
  }
  return -1;
}

function parseTomlEntries(tomlContent) {
  const lines = tomlContent.split('\n');
  const entries = [];
  let current = null;

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
      if (current) entries.push(current);
      current = { key: trimmed.slice(1, -1), name: '', rva: '', sig: '' };
    } else if (current) {
      if (trimmed.startsWith('name =') || trimmed.startsWith('name=')) {
        current.name = trimmed.split('=')[1].trim().replace(/^"|"$/g, '');
      } else if (trimmed.startsWith('rva =') || trimmed.startsWith('rva=')) {
        current.rva = trimmed.split('=')[1].trim().replace(/^"|"$/g, '');
      } else if (trimmed.startsWith('sig =') || trimmed.startsWith('sig=')) {
        current.sig = trimmed.split('=')[1].trim().replace(/^"|"$/g, '');
      }
    }
  }
  if (current) entries.push(current);
  return entries;
}

export function generatePatternToml(templateTomlPath, targetDllPath) {
  const dllBuffer = fs.readFileSync(targetDllPath);
  const dllSha = sha256(dllBuffer);
  const sections = parseSections(dllBuffer);
  const textSection = sections.find(s => s.name === '.text') || sections[0];

  const templateContent = fs.readFileSync(templateTomlPath, 'utf8');
  const templateEntries = parseTomlEntries(templateContent);

  console.log(`Scanning ${templateEntries.length} patterns against ${path.basename(targetDllPath)} (${dllSha})...`);

  const results = [];
  let matchedCount = 0;

  for (const entry of templateEntries) {
    if (!entry.sig) continue;
    let { pattern, mask } = parseSig(entry.sig);
    let fileOffset = scanPattern(
      dllBuffer,
      textSection.pointerToRawData,
      textSection.pointerToRawData + textSection.sizeOfRawData,
      { pattern, mask }
    );

    // If long signature missed due to shifted tail constant, try adaptive shortened prefix (>= 16 bytes)
    if (fileOffset < 0 && pattern.length > 20) {
      const shortLen = Math.min(pattern.length - 8, Math.max(16, Math.floor(pattern.length * 0.75)));
      const shortPattern = pattern.subarray(0, shortLen);
      const shortMask = mask.slice(0, shortLen);
      fileOffset = scanPattern(
        dllBuffer,
        textSection.pointerToRawData,
        textSection.pointerToRawData + textSection.sizeOfRawData,
        { pattern: shortPattern, mask: shortMask }
      );
      if (fileOffset >= 0) {
        console.log(`[ADAPTIVE HIT] ${entry.name}`);
      }
    }

    if (fileOffset >= 0) {
      const rva = fileOffset - textSection.pointerToRawData + textSection.virtualAddress;
      const rvaHex = '0x' + rva.toString(16).toUpperCase();
      results.push({
        key: entry.key,
        name: entry.name,
        rva: rvaHex,
        sig: entry.sig,
      });
      matchedCount++;
    } else {
      console.warn(`[MISS] ${entry.name}`);
    }
  }

  console.log(`Matched ${matchedCount}/${templateEntries.length} patterns!`);

  let outToml = '';
  for (const r of results) {
    outToml += `[${r.key}]\nname = "${r.name}"\nrva = "${r.rva}"\nsig = "${r.sig}"\n\n`;
  }
  return { sha: dllSha, toml: outToml, matchedCount, totalCount: templateEntries.length };
}

async function main() {
  const steamPath = 'C:\\Program Files (x86)\\Steam';
  const patternDir = path.join(steamPath, '_0xolemoncore', 'pattern');

  // 1. steamclient64.dll
  const oldClientToml = path.join(patternDir, '86112382982fa855086f566b2fb8343290e798849029337dcfeecba0d5051b5e.toml');
  const clientDll = path.join(steamPath, 'steamclient64.dll');
  const clientRes = generatePatternToml(oldClientToml, clientDll);

  // Write steamclient pattern to root pattern dir and subdir
  fs.writeFileSync(path.join(patternDir, `${clientRes.sha}.toml`), clientRes.toml, 'utf8');
  fs.mkdirSync(path.join(patternDir, 'steamclient'), { recursive: true });
  fs.writeFileSync(path.join(patternDir, 'steamclient', `${clientRes.sha}.toml`), clientRes.toml, 'utf8');

  // 2. steamui.dll
  const oldUiToml = path.join(patternDir, 'af6ca9193dd6d502fad83d4a51ea29fe156699c2dfcf5739f9f70ca659d2b83d.toml');
  const uiDll = path.join(steamPath, 'steamui.dll');
  const uiRes = generatePatternToml(oldUiToml, uiDll);

  // Write steamui pattern to root pattern dir and subdir
  fs.writeFileSync(path.join(patternDir, `${uiRes.sha}.toml`), uiRes.toml, 'utf8');
  fs.mkdirSync(path.join(patternDir, 'steamui'), { recursive: true });
  fs.writeFileSync(path.join(patternDir, 'steamui', `${uiRes.sha}.toml`), uiRes.toml, 'utf8');

  console.log('Successfully generated and written pattern TOML files to Steam directory!');
}

main().catch(console.error);
