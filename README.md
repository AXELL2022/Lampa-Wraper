# Lampa Desktop (Tauri)

Обёртка [yumata/lampa-source](https://github.com/yumata/lampa-source) в десктоп-приложение на Tauri 2.
Сайт собирается локально и раздаётся из самого приложения (`http://tauri.localhost`), без внешнего хостинга.

## Требования
Node.js, Rust (stable), WebView2 (есть в Windows 10/11).

## Команды
```powershell
npm install
npm run build:lampa   # клонирует (если нужно) и собирает lampa-source -> lampa-source/build/web
npm run dev           # запуск в режиме разработки (сначала один раз build:lampa)
npm run build         # пересобирает Lampa и делает установщики (NSIS + MSI)
```

Результат: `src-tauri/target/release/lampa.exe`, установщики — в `src-tauri/target/release/bundle/`.

## Обновление Lampa
```powershell
git -C lampa-source pull
npm run build
```

## Как устроено
- `scripts/build-lampa.mjs` — upstream gulpfile умеет только watch + browser-sync, поэтому скрипт
  генерирует временную копию gulpfile с одноразовой задачей `build_tauri` (rollup → sass → lang → public → build/web).
  Исходники Lampa не меняются.
- `src-tauri/tauri.conf.json` — `frontendDist` указывает на `lampa-source/build/web`, CSP выключен
  (Lampa ходит на множество внешних API, CDN и в TorrServer по http).
- F11 — полноэкранный режим.
- Lampa всегда загружает `plugins/modification.js`. Tauri на отсутствующий файл отдаёт `index.html`
  (ошибка «Unexpected token '<'»), поэтому сборка кладёт туда заглушку. Свой код для этого хука
  можно положить в `modification.js` в корне проекта: он попадёт в сборку.

## Внешний плеер
Настройки → Плеер → «Тип плеера» = **Внешний**, в поле «Путь к плееру» укажите exe, например
`C:/Program Files/VideoLAN/VLC/vlc.exe`. Отдельно можно выбрать плеер для торрентов и IPTV.

- VLC, MPC-HC / MPC-BE / MPC-QT, KMPlayer: запуск с позиции, где остановились, и сохранение прогресса
  после закрытия (через их HTTP-интерфейс; запросы к нему идут через Rust, чтобы WebView не блокировал их по CORS).
- mpv: то же самое через JSON IPC mpv (`--input-ipc-server=\\.\pipe\lampa-mpv`), включая плейлисты
  сериалов: при переключении серии в mpv прогресс сохраняется для каждой серии.
- Любой другой exe: запускается с URL видео в аргументе.

Как это сделано: `src-tauri/src/init.js` выполняется до Lampa и представляет приложение как NW.js
(`window.nw`, `window.require` с `fs`, `child_process`, `os`, `path`, `net`). Процессы, HTTP-запросы
к плееру и обмен с named pipe mpv выполняет `src-tauri/src/external.rs`. Заодно ссылки «открыть в браузере» открываются в системном браузере.
