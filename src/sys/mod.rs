//! Системный слой WinForge: реестр, исполнитель действий, чтение реальных
//! данных, очистка, сборка образа, журнал отката и проверка прав.
//!
//! Весь слой изолирован от интерфейса: UI вызывает эти функции, а не winapi.

pub mod activation;
pub mod clean;
pub mod engine;
pub mod image;
pub mod installer;
pub mod probe;
pub mod reg;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Права администратора
// ---------------------------------------------------------------------------

/// Запущены ли мы с правами администратора (нужно для изменения системы).
/// Значение не меняется за время работы процесса, поэтому кэшируется.
pub fn is_elevated() -> bool {
    static ELEVATED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ELEVATED.get_or_init(check_elevated)
}

#[cfg(windows)]
fn check_elevated() -> bool {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
fn check_elevated() -> bool {
    false
}

/// ShellExecuteW: `verb` = "open" | "runas". Возвращает успех.
#[cfg(windows)]
fn shell_execute(verb: &str, file: &str, params: Option<&str>) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let verb = wide(verb);
    let file = wide(file);
    let params = params.map(wide);
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    (result as isize) > 32
}

#[cfg(not(windows))]
fn shell_execute(_verb: &str, _file: &str, _params: Option<&str>) -> bool {
    false
}

/// Перезапускает текущую программу с запросом прав администратора.
pub fn relaunch_as_admin() -> bool {
    match std::env::current_exe() {
        Ok(exe) => shell_execute("runas", &exe.to_string_lossy(), None),
        Err(_) => false,
    }
}

/// Открывает папку в Проводнике.
pub fn open_folder(path: &std::path::Path) -> bool {
    shell_execute("open", &path.to_string_lossy(), None)
}

/// Открывает ссылку в браузере по умолчанию (только http/https).
pub fn open_url(url: &str) -> bool {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return false;
    }
    shell_execute("open", url, None)
}

/// Запускает программу с параметрами (без ожидания).
pub fn launch(program: &str, params: &str) -> bool {
    shell_execute("open", program, Some(params))
}

/// Процесс, запущенный от имени администратора: WinForge (обычные права) может
/// ждать его завершения и узнать код выхода, хотя управлять им не может.
pub struct AdminProcess {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

// HANDLE процесса можно опрашивать из любого потока.
unsafe impl Send for AdminProcess {}

impl AdminProcess {
    /// Some(код выхода), если процесс завершился.
    #[cfg(windows)]
    pub fn exit_code(&self) -> Option<u32> {
        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
        unsafe {
            if WaitForSingleObject(self.handle, 0) != WAIT_OBJECT_0 {
                return None;
            }
            let mut code = 0u32;
            GetExitCodeProcess(self.handle, &mut code);
            Some(code)
        }
    }

    #[cfg(not(windows))]
    pub fn exit_code(&self) -> Option<u32> {
        Some(1)
    }
}

#[cfg(windows)]
impl Drop for AdminProcess {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
    }
}

/// Запускает .ps1 от имени администратора без окна (запрос UAC — как обычно).
/// None — пользователь отказал в UAC или запуск не удался.
#[cfg(windows)]
pub fn run_script_hidden_as_admin(script: &std::path::Path, args: &str) -> Option<AdminProcess> {
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let verb = wide("runas");
    let file = wide("powershell.exe");
    let params = wide(&format!(
        "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File \"{}\" {args}",
        script.display()
    ));
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_HIDE;
        if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
            return None;
        }
        Some(AdminProcess { handle: info.hProcess })
    }
}

#[cfg(not(windows))]
pub fn run_script_hidden_as_admin(_script: &std::path::Path, _args: &str) -> Option<AdminProcess> {
    None
}

// ---------------------------------------------------------------------------
// Папки данных
// ---------------------------------------------------------------------------

pub fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("WinForge")
}

pub fn backup_dir() -> PathBuf {
    data_dir().join("backups")
}

// ---------------------------------------------------------------------------
// Журнал отката
// ---------------------------------------------------------------------------

/// Журнал одного применения: набор записей для отмены.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Journal {
    /// Время создания (unix-секунды).
    pub created: u64,
    /// Человекочитаемая метка (что применяли).
    pub label: String,
    pub undos: Vec<engine::Undo>,
}

impl Journal {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            created: now_secs(),
            label: label.into(),
            undos: Vec::new(),
        }
    }

    /// Дата/время для показа пользователю.
    pub fn display_time(&self) -> String {
        fmt_unix(self.created)
    }

    /// Сохраняет журнал в папку бэкапов. Возвращает путь к файлу.
    pub fn save(&self) -> std::io::Result<PathBuf> {
        let dir = backup_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("wf-{}.toml", self.created));
        let text = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&path, text)?;
        Ok(path)
    }
}

impl Journal {
    /// Нужны ли права администратора для отката (HKLM, службы, задачи, компоненты, команды).
    pub fn needs_admin(&self) -> bool {
        self.undos.iter().any(|u| match u {
            engine::Undo::Reg { root, .. } | engine::Undo::KeyCreated { root, .. } => {
                !matches!(root, reg::Root::Hkcu)
            }
            _ => true,
        })
    }
}

/// Все ещё не откаченные журналы (путь, журнал), новейшие первыми.
pub fn list_journals() -> Vec<(PathBuf, Journal)> {
    let mut journals: Vec<(PathBuf, Journal)> = std::fs::read_dir(backup_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "toml").unwrap_or(false))
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            let journal = toml::from_str::<Journal>(&text).ok()?;
            Some((p, journal))
        })
        .collect();
    journals.sort_by(|a, b| b.1.created.cmp(&a.1.created));
    journals
}

/// Помечает журнал откаченным (файл переименовывается и больше не показывается).
pub fn mark_rolled_back(path: &std::path::Path) {
    let _ = std::fs::rename(path, path.with_extension("rolledback"));
}

/// Откатывает журнал (в обратном порядке). Возвращает (успешно, ошибки).
pub fn rollback(journal: &Journal) -> (usize, Vec<String>) {
    let mut ok = 0;
    let mut errors = Vec::new();
    for undo in journal.undos.iter().rev() {
        match engine::revert(undo) {
            Ok(()) => ok += 1,
            Err(e) => errors.push(e),
        }
    }
    (ok, errors)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// unix-секунды → "ГГГГ-ММ-ДД ГГ:ХХ" (локально не учитывается, UTC).
fn fmt_unix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m) = (rem / 3600, (rem % 3600) / 60);
    // Алгоритм civil_from_days (Howard Hinnant).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_format() {
        // 1700000000 = 2023-11-14 22:13:20 UTC
        assert_eq!(fmt_unix(1_700_000_000), "2023-11-14 22:13");
        assert_eq!(fmt_unix(0), "1970-01-01 00:00");
    }
}
