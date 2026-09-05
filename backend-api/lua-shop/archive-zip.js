// Stored ZIP entries keep packaging deterministic without another runtime dependency.
const table = Array.from({ length: 256 }, (_, n) => { for (let i = 0; i < 8; i++) n = n & 1 ? 0xedb88320 ^ (n >>> 1) : n >>> 1; return n >>> 0; });
function crc32(bytes) { let crc = 0xffffffff; for (const byte of bytes) crc = table[(crc ^ byte) & 255] ^ (crc >>> 8); return (crc ^ 0xffffffff) >>> 0; }
function archiveZip(entries) {
  const body = [], central = []; let offset = 0;
  for (const [path, bytes] of [...entries].sort(([a], [b]) => a.localeCompare(b))) {
    const name = Buffer.from(path), crc = crc32(bytes);
    const header = Buffer.alloc(30); header.writeUInt32LE(0x04034b50); header.writeUInt16LE(20, 4); header.writeUInt16LE(0x800, 6);
    header.writeUInt32LE(crc, 14); header.writeUInt32LE(bytes.length, 18); header.writeUInt32LE(bytes.length, 22); header.writeUInt16LE(name.length, 26);
    const dir = Buffer.alloc(46); dir.writeUInt32LE(0x02014b50); dir.writeUInt16LE(20, 4); dir.writeUInt16LE(20, 6); dir.writeUInt16LE(0x800, 8);
    dir.writeUInt32LE(crc, 16); dir.writeUInt32LE(bytes.length, 20); dir.writeUInt32LE(bytes.length, 24); dir.writeUInt16LE(name.length, 28); dir.writeUInt32LE(offset, 42);
    body.push(header, name, bytes); central.push(dir, name); offset += header.length + name.length + bytes.length;
  }
  const directory = Buffer.concat(central), end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50); end.writeUInt16LE(entries.size, 8); end.writeUInt16LE(entries.size, 10); end.writeUInt32LE(directory.length, 12); end.writeUInt32LE(offset, 16);
  return Buffer.concat([...body, directory, end]);
}
module.exports = { archiveZip };
