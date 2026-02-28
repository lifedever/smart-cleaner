import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';

/**
 * Sequential build script for macOS.
 * Builds for each target, finds the generated .tar.gz and .sig updater artifacts,
 * and constructs a consolidated latest.json for the Tauri updater.
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

  try {
    execSync(`npx tauri build --target ${target}`, { stdio: 'inherit' });

    // Tauri v2 puts updater artifacts in bundle/macos/
    const bundleDir = path.join('src-tauri', 'target', target, 'release', 'bundle', 'macos');
    console.log(`\n📂 Checking bundle dir: ${bundleDir}`);

    // List all files in the bundle dir for debugging
    if (fs.existsSync(bundleDir)) {
      const files = fs.readdirSync(bundleDir);
      console.log(`Files found: ${files.join(', ')}`);

      // Find .tar.gz and .sig files
      const tarGzFile = files.find(f => f.endsWith('.tar.gz') && !f.endsWith('.sig'));
      const sigFile = files.find(f => f.endsWith('.tar.gz.sig'));

      if (tarGzFile && sigFile) {
        const signature = fs.readFileSync(path.join(bundleDir, sigFile), 'utf8').trim();
        const tauriPlatform = target.startsWith('aarch64') ? 'darwin-aarch64' : 'darwin-x86_64';
        const downloadUrl = `https://github.com/${repo}/releases/download/${tag}/${tarGzFile}`;

        results.platforms[tauriPlatform] = {
          signature: signature,
          url: downloadUrl
        };

        console.log(`✅ Collected updater info for ${tauriPlatform}`);
        console.log(`   URL: ${downloadUrl}`);
        console.log(`   Signature: ${signature.substring(0, 40)}...`);
      } else {
        console.warn(`⚠️ Missing updater artifacts in ${bundleDir}`);
        console.warn(`   tar.gz: ${tarGzFile || 'NOT FOUND'}`);
        console.warn(`   sig: ${sigFile || 'NOT FOUND'}`);
      }
    } else {
      console.error(`❌ Bundle directory does not exist: ${bundleDir}`);
      // Try finding files anywhere
      try {
        const allFiles = execSync(`find src-tauri/target/${target}/release/bundle -type f -name "*.tar.gz" -o -name "*.sig"`, { encoding: 'utf8' });
        console.log(`Fallback search results:\n${allFiles}`);
      } catch (e) {
        console.error('Fallback search also failed');
      }
    }
  } catch (err) {
    console.error(`❌ Error building for ${target}:`, err.message);
    process.exit(1);
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
