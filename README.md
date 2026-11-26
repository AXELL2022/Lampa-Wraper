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

## Автообновление приложения
При запуске приложение проверяет последний релиз на GitHub
(`https://github.com/AXELL2022/Lampa-Wraper/releases/latest/download/latest.json`).
Если версия новее, спрашивает «Обновить?», скачивает установщик, проверяет его подпись
(публичный ключ лежит в `tauri.conf.json`), ставит и перезапускается. Вручную проверить можно
из трея: «Проверить обновления».

### Один раз: ключ подписи
Приватный ключ: `~/.tauri/lampa-updater.key`, пароль пустой. **Не коммитьте его и не теряйте:** без него
нельзя выпустить обновление, которое примут уже установленные копии.
Добавьте его в GitHub → Settings → Secrets and variables → Actions → секрет
`TAURI_SIGNING_PRIVATE_KEY` (содержимое файла целиком). Локальный `npm run build` подхватит ключ сам.

### Выпуск версии
```powershell
npm run version -- 1.0.1
git commit -am "v1.0.1"
git tag v1.0.1
git push origin HEAD v1.0.1
```
Workflow `.github/workflows/release.yml` соберёт свежую Lampa и приложение, подпишет их и опубликует
релиз с `latest.json`. Установленные копии предложат обновиться при следующем запуске.

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
- Трей (`src-tauri/src/tray.rs`, по образцу Anivox-Wraper): левый клик скрывает или показывает окно,
  в меню «Открыть», «Сворачивать в трей», «Проверить обновления» и «Выход». При включённой галочке крестик прячет окно в трей.
  Настройка хранится в `%APPDATA%\com.lampa.desktop\tray.json`, по умолчанию включена.
  Повторный запуск приложения показывает уже открытое окно (single-instance).
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
