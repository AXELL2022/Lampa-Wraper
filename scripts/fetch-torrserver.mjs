// Скачивает TorrServer (github.com/YouROK/TorrServer) в src-tauri/binaries как sidecar Tauri.
// Tauri ищет sidecar по имени с target triple и кладёт его рядом с lampa.exe как TorrServer.exe.
//   node scripts/fetch-torrserver.mjs           — скачать, если файла ещё нет
//   node scripts/fetch-torrserver.mjs --force   — перекачать (обновить TorrServer)
// Версия: последний стабильный релиз ветки MatriX (ветка Silo экспериментальная, с другим CLI);
// конкретный тег можно задать в TORRSERVER_VERSION (например MatriX.151).
import { existsSync, mkdirSync, renameSync, statSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dir = join(root, 'src-tauri', 'binaries');
const target = join(dir, 'TorrServer-x86_64-pc-windows-msvc.exe');
const force = process.argv.includes('--force');

if (existsSync(target) && statSync(target).size > 1_000_000 && !force) {
  console.log('TorrServer уже есть:', target);
  process.exit(0);
}

const ASSET = 'TorrServer-windows-amd64.exe';
let tag = process.env.TORRSERVER_VERSION;
if (!tag) {
  const headers = { 'User-Agent': 'lampa-wraper-build', Accept: 'application/vnd.github+json' };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const r = await fetch('https://api.github.com/repos/YouROK/TorrServer/releases?per_page=30', { headers });
  if (!r.ok) {
    console.error(`Не удалось получить список релизов TorrServer: HTTP ${r.status}`);
    process.exit(1);
  }
  const rel = (await r.json()).find(
    (x) => /^MatriX\./.test(x.tag_name) && !x.prerelease && !x.draft && x.assets.some((a) => a.name === ASSET)
  );
  if (!rel) {
    console.error('Не найден релиз TorrServer MatriX с ' + ASSET);
    process.exit(1);
  }
  tag = rel.tag_name;
}
const url = `https://github.com/YouROK/TorrServer/releases/download/${tag}/${ASSET}`;

console.log('Скачиваю TorrServer:', url);
const res = await fetch(url, { redirect: 'follow' });
if (!res.ok) {
  console.error(`Не удалось скачать TorrServer: HTTP ${res.status}`);
  process.exit(1);
}
const buf = Buffer.from(await res.arrayBuffer());
// Проверка, что это PE-файл, а не HTML-страница ошибки
if (buf.length < 1_000_000 || buf[0] !== 0x4d || buf[1] !== 0x5a) {
  console.error('Скачанный файл не похож на exe TorrServer');
  process.exit(1);
}
mkdirSync(dir, { recursive: true });
writeFileSync(target + '.tmp', buf);
renameSync(target + '.tmp', target);
console.log(`TorrServer сохранён (${(buf.length / 1048576).toFixed(1)} МБ):`, target);
