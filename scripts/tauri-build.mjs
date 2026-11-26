// Локальная сборка: подставляет ключ подписи обновлений из ~/.tauri/lampa-updater.key,
// если TAURI_SIGNING_PRIVATE_KEY не задан (без ключа tauri build не создаст .sig и упадёт).
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

const env = { ...process.env };
const keyPath = join(homedir(), '.tauri', 'lampa-updater.key');
if (!env.TAURI_SIGNING_PRIVATE_KEY && existsSync(keyPath)) {
  env.TAURI_SIGNING_PRIVATE_KEY = keyPath;
  env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ??= '';
}
if (!env.TAURI_SIGNING_PRIVATE_KEY) {
  console.warn(`Нет ключа подписи (${keyPath}): обновления не будут подписаны, сборка упадёт на этапе подписи.`);
}

const r = spawnSync('npx', ['tauri', 'build', ...process.argv.slice(2)], { stdio: 'inherit', env, shell: true });
process.exit(r.status ?? 1);
