const fs = require('fs');
const path = require('path');

console.log("==========================================");
console.log("   007 Launcher - Depot Patch Merger");
console.log("==========================================\n");

if (process.argv.length < 6) {
  console.log("Usage: node merge_depot_patch.js <old_manifest.json> <patch_manifest.json> <old_catalog.json> <patch_catalog.json>");
  console.log("\nExample:");
  console.log("  node merge_depot_patch.js C:\\GameOut\\manifests\\v1.0.json C:\\PatchOut\\manifests\\v1.1.json C:\\GameOut\\catalog.json C:\\PatchOut\\catalog.json");
  process.exit(1);
}

const oldManifestPath = process.argv[2];
const patchManifestPath = process.argv[3];
const oldCatalogPath = process.argv[4];
const patchCatalogPath = process.argv[5];

try {
  console.log("[*] Reading old and new manifests/catalogs...");
  const oldManifest = JSON.parse(fs.readFileSync(oldManifestPath, 'utf8'));
  const patchManifest = JSON.parse(fs.readFileSync(patchManifestPath, 'utf8'));
  const oldCatalog = JSON.parse(fs.readFileSync(oldCatalogPath, 'utf8'));
  const patchCatalog = JSON.parse(fs.readFileSync(patchCatalogPath, 'utf8'));

  // 1. Merge manifest files
  console.log("[*] Merging file entries...");
  const filesMap = new Map();
  oldManifest.files.forEach(f => filesMap.set(f.path, f));
  
  let newFilesCount = 0;
  let updatedFilesCount = 0;
  patchManifest.files.forEach(f => {
    if (filesMap.has(f.path)) {
        updatedFilesCount++;
    } else {
        newFilesCount++;
    }
    filesMap.set(f.path, f);
  }); 

  const mergedFiles = Array.from(filesMap.values());
  const mergedTotalSize = mergedFiles.reduce((acc, f) => acc + f.size, 0);

  const newManifest = {
    ...oldManifest,
    version: patchManifest.version,
    createdAt: patchManifest.createdAt,
    totalSize: mergedTotalSize,
    files: mergedFiles,
    signature: null // invalidate old signature
  };

  const newManifestPath = path.join(path.dirname(oldManifestPath), `${patchManifest.version}.json`);
  fs.writeFileSync(newManifestPath, JSON.stringify(newManifest, null, 2));
  console.log(`[+] Saved merged manifest to ${newManifestPath}`);
  console.log(`    - Updated files: ${updatedFilesCount}`);
  console.log(`    - New files: ${newFilesCount}`);
  console.log(`    - Total files in new manifest: ${mergedFiles.length}`);

  // 2. Merge catalog packs
  console.log("\n[*] Merging catalog packs...");
  const oldPacksMap = new Map();
  oldCatalog.packs.forEach(p => oldPacksMap.set(p.id, p));
  let newPacksCount = 0;
  patchCatalog.packs.forEach(p => {
      if (!oldPacksMap.has(p.id)) newPacksCount++;
      oldPacksMap.set(p.id, p);
  });

  // 3. Update catalog version entry
  const newVersionEntry = {
    version: patchManifest.version,
    manifestPath: `manifests/${patchManifest.version}.json`,
    totalSize: mergedTotalSize,
    fileCount: mergedFiles.length,
    chunkCount: mergedFiles.reduce((acc, f) => acc + f.chunks.length, 0),
    createdAt: patchManifest.createdAt
  };

  // Check if version already exists to replace or append
  const existingVersionIndex = oldCatalog.versions.findIndex(v => v.version === patchManifest.version);
  if (existingVersionIndex >= 0) {
    oldCatalog.versions[existingVersionIndex] = newVersionEntry;
    console.log(`[*] Overwrote existing version ${patchManifest.version} in catalog.`);
  } else {
    oldCatalog.versions.push(newVersionEntry);
    console.log(`[*] Added new version ${patchManifest.version} to catalog.`);
  }

  const newCatalog = {
    ...oldCatalog,
    latestVersion: patchManifest.version,
    packs: Array.from(oldPacksMap.values()),
    signature: null // invalidate signature
  };

  fs.writeFileSync(oldCatalogPath, JSON.stringify(newCatalog, null, 2));
  console.log(`[+] Saved updated catalog to ${oldCatalogPath}`);
  console.log(`    - Added ${newPacksCount} new packs.`);
  
  console.log("\n==========================================");
  console.log("   MERGE SUCCESSFUL!");
  console.log("==========================================");
  console.log("Don't forget to copy the generated .bin packs from your patch output directory to your main packs/ directory before uploading to Hugging Face.");

} catch (err) {
  console.error("\n[!] ERROR:", err.message);
  process.exit(1);
}
