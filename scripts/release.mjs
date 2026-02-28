import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';

/**
 * Sequential build script for macOS.
 * 
 * Strategy: Build in two phases per target to avoid DMG bundling failures
 * blocking updater artifact generation.
 * 
 * Phase 1: `--bundles app` → generates .app, .tar.gz, .tar.gz.sig (updater artifacts)
 * Phase 2: `--bundles dmg` → generates .dmg (installer, allowed to fail)
 */

const targets = ['aarch64-apple-darwin', 'x86_64-apple-darwin'];
const version = JSON.parse(fs.readFileSync('package.json', 'utf8')).version;

// This will be our final updater JSON
const results = {
  version: `v${version}`,
  notes: `Release v${version}`,
  pub_date: new Date().toISOString(),
  platforms: {}
};

// GitHub repo for constructing download URLs
const repo = 'lifedever/smart-cleaner';
const tag = `v${version}`;

for (const target of targets) {
  console.log(`\n🚀 Building for target: ${target}...\n`);

  // ── Phase 1: Build .app + updater artifacts (must succeed) ──
  try {
    console.log(`📦 Phase 1: Building app bundle for ${target}...`);
    execSync(`npx tauri build --target ${target} --bundles app`, { stdio: 'inherit' });
  } catch (err) {
    console.error(`❌ Error building app for ${target}:`, err.message);
    process.exit(1);
  }

  // ── Phase 2: Build DMG (allowed to fail) ──
  try {
    console.log(`\n💿 Phase 2: Building DMG for ${target}...`);
    execSync(`npx tauri build --target ${target} --bundles dmg`, { stdio: 'inherit' });
    console.log(`✅ DMG created for ${target}`);
  } catch (err) {
    console.warn(`⚠️ DMG creation failed for ${target} (non-fatal): ${err.message}`);
  }

  // ── Collect updater artifacts ──
  const bundleDir = path.join('src-tauri', 'target', target, 'release', 'bundle', 'macos');
  console.log(`\n📂 Checking bundle dir: ${bundleDir}`);

  if (fs.existsSync(bundleDir)) {
    const files = fs.readdirSync(bundleDir);
    console.log(`Files found: ${files.join(', ')}`);

    // Find .tar.gz and .sig files
    const tarGzFile = files.find(f => f.endsWith('.tar.gz') && !f.endsWith('.sig'));
    const sigFile = files.find(f => f.endsWith('.tar.gz.sig'));

    if (tarGzFile && sigFile) {
      const signature = fs.readFileSync(path.join(bundleDir, sigFile), 'utf8').trim();
      const tauriPlatform = target.startsWith('aarch64') ? 'darwin-aarch64' : 'darwin-x86_64';
      const arch = target.startsWith('aarch64') ? 'aarch64' : 'x86_64';

      // Rename tar.gz to include arch suffix and replace spaces with underscores
      // to avoid: 1) both architectures overwriting each other (same filename)
      //           2) GitHub converting spaces to dots causing 404
      const baseName = tarGzFile.replace('.app.tar.gz', '');
      const safeBaseName = baseName.replace(/ /g, '_');
      const renamedTarGz = `${safeBaseName}_${arch}.app.tar.gz`;

      fs.copyFileSync(
        path.join(bundleDir, tarGzFile),
        path.join(bundleDir, renamedTarGz)
      );

      const downloadUrl = `https://github.com/${repo}/releases/download/${tag}/${renamedTarGz}`;

      results.platforms[tauriPlatform] = {
        signature: signature,
        url: downloadUrl
      };

      console.log(`✅ Collected updater info for ${tauriPlatform}`);
      console.log(`   Renamed: ${tarGzFile} → ${renamedTarGz}`);
      console.log(`   URL: ${downloadUrl}`);
      console.log(`   Signature: ${signature.substring(0, 40)}...`);
    } else {
      console.warn(`⚠️ Missing updater artifacts in ${bundleDir}`);
      console.warn(`   tar.gz: ${tarGzFile || 'NOT FOUND'}`);
      console.warn(`   sig: ${sigFile || 'NOT FOUND'}`);
    }
  } else {
    console.error(`❌ Bundle directory does not exist: ${bundleDir}`);
    try {
      const allFiles = execSync(`find src-tauri/target/${target}/release/bundle -type f -name "*.tar.gz" -o -name "*.sig"`, { encoding: 'utf8' });
      console.log(`Fallback search results:\n${allFiles}`);
    } catch (e) {
      console.error('Fallback search also failed');
    }
  }
}

// Write the combined JSON
const outputPath = 'latest.json';
fs.writeFileSync(outputPath, JSON.stringify(results, null, 2));
console.log(`\n✨ Consolidated latest.json created at ${outputPath}`);
console.log(JSON.stringify(results, null, 2));

// Validate
const platformCount = Object.keys(results.platforms).length;
if (platformCount === 0) {
  console.error('\n❌ FATAL: No platform signatures were collected! The updater will not work.');
  process.exit(1);
} else {
  console.log(`\n🎉 Successfully collected ${platformCount} platform(s).`);
}
