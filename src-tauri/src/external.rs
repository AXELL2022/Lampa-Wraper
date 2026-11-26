//! Запуск внешнего видеоплеера и опрос его HTTP-интерфейса (VLC / MPC / KMPlayer).
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
pub struct Processes {
    next_id: AtomicU32,
    children: Mutex<HashMap<u32, Arc<Mutex<Child>>>>,
}

#[derive(Clone, Serialize)]
struct ExitPayload {
    id: u32,
    code: Option<i32>,
}

#[tauri::command]
pub fn lampa_spawn(
    app: AppHandle,
    state: State<'_, Processes>,
    path: String,
    args: Vec<String>,
) -> Result<u32, String> {
    if path.trim().is_empty() || !Path::new(&path).is_file() {
        return Err(format!("not found: {path}"));
    }

    let mut cmd = Command::new(&path);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = Arc::new(Mutex::new(cmd.spawn().map_err(|e| e.to_string())?));
    let id = state.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    state.children.lock().unwrap().insert(id, child.clone());

    // Ждём завершения в отдельном потоке, не держа lock постоянно (чтобы работал kill)
    thread::spawn(move || {
        let code = loop {
            match child.lock().unwrap().try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) => {}
                Err(_) => break None,
            }
            thread::sleep(Duration::from_millis(300));
        };
        let _ = app.emit("lampa-process-exit", ExitPayload { id, code });
    });

    Ok(id)
}

#[tauri::command]
pub fn lampa_kill(state: State<'_, Processes>, id: u32) {
    if let Some(child) = state.children.lock().unwrap().remove(&id) {
        let _ = child.lock().unwrap().kill();
    }
}

#[derive(Serialize)]
pub struct HttpResult {
    status: u16,
    body: String,
}

/// GET к HTTP-интерфейсу плеера на localhost (из WebView его блокирует CORS).
#[tauri::command]
pub async fn lampa_local_get(
    port: u16,
    path: String,
    headers: HashMap<String, String>,
) -> Result<HttpResult, String> {
    tauri::async_runtime::spawn_blocking(move || local_get(port, &path, &headers))
        .await
        .map_err(|e| e.to_string())?
}

fn local_get(port: u16, path: &str, headers: &HashMap<String, String>) -> Result<HttpResult, String> {
    if !path.starts_with('/') || path.contains(['\r', '\n']) {
        return Err("bad path".into());
    }
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    let mut req = format!("GET {path} HTTP/1.0\r\nHost: localhost:{port}\r\nConnection: close\r\n");
    for (k, v) in headers {
        if k.contains(['\r', '\n', ':']) || v.contains(['\r', '\n']) {
            continue;
        }
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;

    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("bad response")?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let mut body = raw[split + 4..].to_vec();

    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or("bad status line")?;

    if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        body = dechunk(&body);
    }

    Ok(HttpResult {
        status,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

/// Обмен с JSON IPC mpv (named pipe на Windows, unix socket на остальных):
/// отправляет `data` (строки JSON с request_id) и читает ответы, пока не придут все request_id.
#[tauri::command]
pub async fn lampa_ipc_exchange(path: String, data: String, timeout_ms: Option<u64>) -> Result<String, String> {
    let ids: HashSet<i64> = data
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v.get("request_id").and_then(|id| id.as_i64()))
        .collect();
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(2000));

    // Чтение из pipe блокирующее и без таймаута — работаем в отдельном потоке
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(ipc_exchange(&path, &data, ids));
    });
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(timeout))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|_| "mpv IPC timeout".to_string())?
}

fn ipc_exchange(path: &str, data: &str, mut ids: HashSet<i64>) -> Result<String, String> {
    #[cfg(windows)]
    let mut conn = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let mut conn = {
        let s = std::os::unix::net::UnixStream::connect(path).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(Duration::from_secs(5))).ok();
        s
    };

    conn.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    conn.flush().ok();

    let mut out = String::new();
    let mut buf = [0u8; 4096];
    let mut pending = String::new();
    while !ids.is_empty() {
        let n = conn.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        pending.push_str(&String::from_utf8_lossy(&buf[..n]));
        while let Some(pos) = pending.find('\n') {
            let line: String = pending.drain(..=pos).collect();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
                if let Some(id) = v.get("request_id").and_then(|id| id.as_i64()) {
                    ids.remove(&id);
                }
            }
            out.push_str(&line);
        }
    }
    Ok(out)
}

fn dechunk(mut data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(pos) = data.windows(2).position(|w| w == b"\r\n") {
        let size_str = String::from_utf8_lossy(&data[..pos]);
        let size = usize::from_str_radix(size_str.split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
        data = &data[pos + 2..];
        if size == 0 || size > data.len() {
            break;
        }
        out.extend_from_slice(&data[..size]);
        data = &data[(size + 2).min(data.len())..];
    }
    out
}
