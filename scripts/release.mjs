import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';

/**
 * Sequential build script for macOS to produce separate DMGs 
 * and a consolidated latest.json for the updater.
 */

const targets = ['aarch64-apple-darwin', 'x86_64-apple-darwin'];
const results = {
  version: '',
  notes: '',
  pub_date: new Date().toISOString(),
  platforms: {}
};

const version = JSON.parse(fs.readFileSync('package.json', 'utf8')).version;
results.version = version;
results.notes = `Release v${version}`;

for (const target of targets) {
  console.log(`\n🚀 Building for target: ${target}...\n`);
  
  try {
    // Run building
    execSync(`npx tauri build --target ${target}`, { stdio: 'inherit' });
    
    // List some files for debugging
    const bundleDir = path.join('src-tauri', 'target', target, 'release', 'bundle');
    console.log(`Checking bundle dir: ${bundleDir}`);
    
    const tauriPlatform = target.startsWith('aarch64') ? 'darwin-aarch64' : 'darwin-x86_64';
    const jsonPath = path.join(bundleDir, 'updater', 'latest.json');
    const altJsonPath = path.join(bundleDir, 'macos', 'latest.json');
    
    let activePath = fs.existsSync(jsonPath) ? jsonPath : (fs.existsSync(altJsonPath) ? altJsonPath : null);
    
    if (activePath) {
      console.log(`✅ Found updater JSON at: ${activePath}`);
      const data = JSON.parse(fs.readFileSync(activePath, 'utf8'));
      if (data.platforms && data.platforms[tauriPlatform]) {
        results.platforms[tauriPlatform] = data.platforms[tauriPlatform];
        console.log(`✅ Collected signature for ${tauriPlatform}`);
      }
    } else {
      console.warn(`⚠️ Warning: No latest.json found for ${target} in ${bundleDir}`);
      // Find all json files to help debug
      try {
        const allFiles = execSync(`find ${bundleDir} -name "*.json"`, { encoding: 'utf8' });
        console.log(`Potential JSON files found:\n${allFiles}`);
      } catch (e) {}
    }
  } catch (err) {
    console.error(`❌ Error building for ${target}:`, err.message);
    process.exit(1);
  }
}

// Write the combined JSON
fs.writeFileSync('latest.json', JSON.stringify(results, null, 2));
console.log('\n✨ Consolidated latest.json created successfully!\n');
console.log(JSON.stringify(results, null, 2));
