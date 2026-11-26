// Выполняется в каждой странице до загрузки Lampa.
// Делает Tauri похожим на NW.js для Lampa: десктоп-платформа, внешний плеер, открытие ссылок.
(function () {
    if (window.top !== window || window.__lampaTauri) return;
    window.__lampaTauri = true;

    function invoke(cmd, args) {
        return window.__TAURI__.core.invoke(cmd, args);
    }

    var processes = {};

    // ---- Объект, совместимый с ChildProcess (on/once/kill) ----
    function TauriProcess(path, args) {
        var self = this;
        self._handlers = {};
        self._id = null;
        self.pid = null;
        self.killed = false;
        self.exitCode = null;

        invoke('lampa_spawn', { path: String(path), args: (args || []).map(String) })
            .then(function (id) {
                self._id = id;
                self.pid = id;
                processes[id] = self;
                if (self.killed) invoke('lampa_kill', { id: id });
                self._emit('spawn');
            })
            .catch(function (err) {
                console.error('Player', 'spawn error:', err);
                try {
                    if (window.Lampa && Lampa.Noty) {
                        var msg = Lampa.Lang ? Lampa.Lang.translate('player_not_found') : 'Player not found';
                        Lampa.Noty.show(msg + ': ' + path);
                    }
                } catch (e) {}
                self._emit('error', new Error(String(err)));
            });
    }
    TauriProcess.prototype.on = function (ev, fn) {
        (this._handlers[ev] = this._handlers[ev] || []).push(fn);
        return this;
    };
    TauriProcess.prototype.addListener = TauriProcess.prototype.on;
    TauriProcess.prototype.off = function (ev, fn) {
        var list = this._handlers[ev] || [];
        var i = list.indexOf(fn);
        if (i >= 0) list.splice(i, 1);
        return this;
    };
    TauriProcess.prototype.removeListener = TauriProcess.prototype.off;
    TauriProcess.prototype.once = function (ev, fn) {
        var self = this;
        function w() { self.off(ev, w); fn.apply(self, arguments); }
        return this.on(ev, w);
    };
    TauriProcess.prototype._emit = function (ev) {
        var args = Array.prototype.slice.call(arguments, 1);
        (this._handlers[ev] || []).slice().forEach(function (fn) {
            try { fn.apply(null, args); } catch (e) { console.error(e); }
        });
    };
    TauriProcess.prototype.kill = function () {
        this.killed = true;
        if (this._id !== null) invoke('lampa_kill', { id: this._id });
        return true;
    };

    function subscribe() {
        if (!window.__TAURI__ || !window.__TAURI__.event) return setTimeout(subscribe, 50);
        window.__TAURI__.event.listen('lampa-process-exit', function (e) {
            var p = processes[e.payload.id];
            if (!p) return;
            delete processes[e.payload.id];
            p.exitCode = e.payload.code;
            p._emit('exit', e.payload.code, null);
            p._emit('close', e.payload.code, null);
        });
    }
    subscribe();

    // ---- Сокет к JSON IPC mpv (минимум net.Socket: on/write/destroy) ----
    // Записи одного тика копятся и уходят в Rust одним обменом; ответы приходят как 'data'.
    function IpcSocket(path) {
        var self = this;
        self._handlers = {};
        self._out = '';
        self._timer = null;
        self.destroyed = false;
        setTimeout(function () { if (!self.destroyed) self._emit('connect'); }, 0);
    }
    ['on', 'addListener', 'off', 'removeListener', 'once', '_emit'].forEach(function (k) {
        IpcSocket.prototype[k] = TauriProcess.prototype[k];
    });
    IpcSocket.prototype.write = function (chunk) {
        var self = this;
        if (self.destroyed) return false;
        self._out += String(chunk);
        clearTimeout(self._timer);
        self._timer = setTimeout(function () {
            var data = self._out;
            self._out = '';
            invoke('lampa_ipc_exchange', { path: self._path, data: data, timeoutMs: 2000 })
                .then(function (resp) { if (!self.destroyed) self._emit('data', resp); })
                .catch(function (err) { if (!self.destroyed) self._emit('error', new Error(String(err))); });
        }, 0);
        return true;
    };
    IpcSocket.prototype.end = IpcSocket.prototype.destroy = function () {
        this.destroyed = true;
        clearTimeout(this._timer);
        return this;
    };

    var isWin = /windows/i.test(navigator.userAgent);
    var isMac = /macintosh|mac os/i.test(navigator.userAgent);

    // ---- window.require: только то, что использует Lampa ----
    var modules = {
        os: {
            platform: function () { return isWin ? 'win32' : isMac ? 'darwin' : 'linux'; },
            tmpdir: function () { return isWin ? 'C:\\Windows\\Temp' : '/tmp'; }
        },
        path: {
            join: function () {
                return Array.prototype.slice.call(arguments).join(isWin ? '\\' : '/').replace(/[\\/]+/g, isWin ? '\\' : '/');
            }
        },
        net: {
            connect: function (path) { var s = new IpcSocket(path); s._path = String(path); return s; }
        },
        // Синхронно проверить файл из WebView нельзя: проверка делается в Rust при запуске,
        // а при ошибке показывается уведомление «плеер не найден».
        fs: { existsSync: function (p) { return typeof p === 'string' && p.trim().length > 0; } },
        child_process: { spawn: function (path, args) { return new TauriProcess(path, args); } }
    };
    window.require = function (name) {
        if (modules[name]) return modules[name];
        throw new Error("Cannot find module '" + name + "'");
    };

    // ---- Представляемся NW.js: Lampa включит десктоп-режим и «Внешний плеер» ----
    window.nw = {
        Shell: {
            openExternal: function (url) { invoke('plugin:opener|open_url', { url: String(url) }); }
        }
    };

    // ---- HTTP-интерфейс плеера (VLC/MPC/KMPlayer) на localhost: в обход CORS через Rust ----
    var nativeFetch = window.fetch.bind(window);
    var playerApi = /^http:\/\/(localhost|127\.0\.0\.1):(\d+)(\/(requests\/status\.json|variables\.html|status\.html)(\?.*)?)$/;
    window.fetch = function (input, init) {
        var url = typeof input === 'string' ? input : (input && input.url) || '';
        var m = playerApi.exec(url);
        if (!m) return nativeFetch(input, init);

        var headers = {};
        var h = init && init.headers;
        if (h) {
            if (typeof Headers !== 'undefined' && h instanceof Headers) h.forEach(function (v, k) { headers[k] = v; });
            else Object.keys(h).forEach(function (k) { headers[k] = String(h[k]); });
        }
        return invoke('lampa_local_get', { port: parseInt(m[2], 10), path: m[3], headers: headers })
            .then(function (r) { return new Response(r.body, { status: r.status }); })
            .catch(function (err) { throw new TypeError('Failed to fetch: ' + err); });
    };

    // ---- F11 — полноэкранный режим ----
    document.addEventListener('keydown', function (e) {
        if (e.key === 'F11') {
            e.preventDefault();
            if (document.fullscreenElement) document.exitFullscreen();
            else document.documentElement.requestFullscreen();
        }
    }, true);
})();
