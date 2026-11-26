//! Встроенный TorrServer (MatriX): лежит рядом с lampa.exe как TorrServer.exe (sidecar).
//!
//! Запускается скрыто при старте приложения на 127.0.0.1:8090, база и лог — в
//! %APPDATA%\com.lampa.desktop\torrserver. Процесс привязан к Job Object с
//! KILL_ON_JOB_CLOSE: Windows убьёт его, как только завершится Lampa — при «Выход»,
//! установке обновления или падении. Если порт уже занят другим TorrServer, свой не запускаем.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

pub const PORT: u16 = 8090;

#[derive(Default)]
pub struct TorrServer {
    child: Mutex<Option<Child>>,
    last_error: Mutex<Option<String>>,
}

#[derive(Serialize)]
pub struct Status {
    /// running — отвечает наш процесс; external — отвечает чужой TorrServer на порту;
    /// starting — процесс жив, но ещё не отвечает; stopped — не работает
    state: &'static str,
    version: Option<String>,
    port: u16,
    error: Option<String>,
}

fn exe_path() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?.parent()?.join("TorrServer.exe");
    p.exists().then_some(p)
}

/// GET /echo → версия TorrServer, если он отвечает на порту.
fn echo() -> Option<String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(700)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(1500))).ok()?;
    s.write_all(b"GET /echo HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n").ok()?;
    let mut buf = Vec::new();
    let _ = s.take(64 * 1024).read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf);
    let (head, body) = text.split_once("\r\n\r\n")?;
    // 401 тоже значит, что сервер жив (включена авторизация)
    let ok = head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200");
    let auth = head.contains(" 401 ");
    if ok {
        Some(body.trim().to_string())
    } else if auth {
        Some(String::new())
    } else {
        None
    }
}

impl TorrServer {
    fn alive(&self) -> bool {
        let mut g = self.child.lock().unwrap();
        match g.as_mut() {
            Some(c) => match c.try_wait() {
                Ok(None) => true,
                Ok(Some(code)) => {
                    *self.last_error.lock().unwrap() = Some(format!("TorrServer завершился ({code})"));
                    *g = None;
                    false
                }
                Err(_) => false,
            },
            None => false,
        }
    }

    fn start(&self, app: &AppHandle) -> Result<(), String> {
        if self.alive() || echo().is_some() {
            return Ok(());
        }
        let exe = exe_path().ok_or("TorrServer.exe не найден рядом с приложением")?;
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("torrserver");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

        let mut cmd = Command::new(exe);
        cmd.arg("--port")
            .arg(PORT.to_string())
            .arg("--ip")
            .arg("127.0.0.1")
            .arg("--path")
            .arg(&dir)
            .arg("--logpath")
            .arg(dir.join("torrserver.log"))
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let child = cmd.spawn().map_err(|e| {
            let msg = format!("Не удалось запустить TorrServer: {e}");
            *self.last_error.lock().unwrap() = Some(msg.clone());
            msg
        })?;
        #[cfg(windows)]
        job::assign(&child);
        *self.last_error.lock().unwrap() = None;
        *self.child.lock().unwrap() = Some(child);
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub fn start_in_background(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = app.state::<TorrServer>().start(&app) {
            eprintln!("{e}");
        }
    });
}

#[tauri::command]
pub async fn torrserver_status(state: State<'_, TorrServer>) -> Result<Status, String> {
    let alive = state.alive();
    let version = tauri::async_runtime::spawn_blocking(echo).await.map_err(|e| e.to_string())?;
    let st = match (&version, alive) {
        (Some(_), true) => "running",
        (Some(_), false) => "external",
        (None, true) => "starting",
        (None, false) => "stopped",
    };
    Ok(Status {
        state: st,
        version: version.filter(|v| !v.is_empty()),
        port: PORT,
        error: state.last_error.lock().unwrap().clone(),
    })
}

#[tauri::command]
pub async fn torrserver_restart(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let ts = app.state::<TorrServer>();
        ts.stop();
        ts.start(&app)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use std::sync::OnceLock;

    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    struct Job(isize);
    // Хендл живёт до конца процесса; закрывает его сама ОС при выходе → TorrServer убит
    static JOB: OnceLock<Option<Job>> = OnceLock::new();

    fn job() -> Option<isize> {
        JOB.get_or_init(|| unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if h.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                h,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            (ok != 0).then_some(Job(h as isize))
        })
        .as_ref()
        .map(|j| j.0)
    }

    pub fn assign(child: &Child) {
        if let Some(h) = job() {
            unsafe {
                AssignProcessToJobObject(h as _, child.as_raw_handle() as _);
            }
        }
    }
}
