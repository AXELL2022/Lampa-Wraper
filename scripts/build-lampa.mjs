// Собирает web-версию Lampa из ./lampa-source в ./lampa-source/build/web
// Upstream gulpfile не экспортирует одноразовую сборку (только watch + browser-sync),
// поэтому генерируем его копию с дополнительной задачей build_tauri и запускаем её.
import { readFileSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const lampa = join(root, 'lampa-source');

if (!existsSync(lampa)) {
  execSync('git clone --depth 1 https://github.com/yumata/lampa-source.git lampa-source', { cwd: root, stdio: 'inherit' });
}
if (!existsSync(join(lampa, 'node_modules'))) {
  execSync('npm install --no-audit --no-fund', { cwd: lampa, stdio: 'inherit' });
}

const original = readFileSync(join(lampa, 'gulpfile.js'), 'utf8');
const extra = `

// ---- added by lampa-wraper (Tauri) ----
function wait_ms(ms){ return function wait(done){ setTimeout(done, ms) } }
// merge() вызывает done() до окончания rollup — ждём, пока dest/app.js появится и перестанет меняться
function wait_bundle(done){
    let last = -1, stable = 0, started = Date.now();
    let t = setInterval(()=>{
        let size = fs.existsSync(dstFolder+'app.js') ? fs.statSync(dstFolder+'app.js').size : -1;
        if(size > 0 && size === last) stable++; else stable = 0;
        last = size;
        if(stable >= 3){ clearInterval(t); done(); }
        else if(Date.now() - started > 600000){ clearInterval(t); done(new Error('rollup timeout')); }
    }, 1000);
}
exports.build_tauri = series(
    merge, wait_bundle, plugins, sass_task, lang_task, sync_web, build_web, wait_ms(5000)
);
`;
const tmpGulpfile = join(lampa, 'gulpfile.tauri.js');
writeFileSync(tmpGulpfile, original + extra);

rmSync(join(lampa, 'dest', 'app.js'), { force: true });
try {
  execSync('npx gulp --gulpfile gulpfile.tauri.js --cwd . build_tauri', { cwd: lampa, stdio: 'inherit' });
} finally {
  rmSync(tmpGulpfile, { force: true });
}

const web = join(lampa, 'build', 'web');

// Lampa всегда грузит ./plugins/modification.js (хук для своих модификаций). На веб-сервере
// его отсутствие даёт тихий 404, а Tauri вместо 404 отдаёт index.html → «Unexpected token '<'».
// Поэтому кладём пустой файл; свой код можно положить в ./modification.js рядом с package.json.
const customMod = join(root, 'modification.js');
writeFileSync(
  join(web, 'plugins', 'modification.js'),
  existsSync(customMod) ? readFileSync(customMod, 'utf8') : '// Свои модификации Lampa: создайте modification.js в корне lampa-wraper\n'
);
for (const f of ['index.html', 'app.js', 'css/app.css']) {
  if (!existsSync(join(web, f))) {
    console.error(`Сборка не удалась: нет ${f} в ${web}`);
    process.exit(1);
  }
}
console.log('Lampa собрана в', web);
