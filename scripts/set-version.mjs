// npm run version -- 1.0.1  → версия в package.json, tauri.conf.json и Cargo.toml
import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const v = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(v || '')) {
  console.error('Использование: npm run version -- 1.2.3');
  process.exit(1);
}
const root = join(dirname(fileURLToPath(import.meta.url)), '..');

for (const f of ['package.json', 'src-tauri/tauri.conf.json']) {
  const p = join(root, f);
  const j = JSON.parse(readFileSync(p, 'utf8'));
  j.version = v;
  writeFileSync(p, JSON.stringify(j, null, 2) + '\n');
}
const cargo = join(root, 'src-tauri/Cargo.toml');
writeFileSync(cargo, readFileSync(cargo, 'utf8').replace(/^version = ".*?"/m, `version = "${v}"`));

console.log(`Версия ${v}. Дальше:\n  git add -A\n  git commit -m "v${v}"\n  git tag v${v}\n  git push origin HEAD v${v}`);
