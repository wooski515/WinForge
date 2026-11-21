// В release-сборке не показываем чёрное консольное окно рядом с программой.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod catalog;
mod i18n;
mod profile;
mod run;
mod sys;
mod theme;
mod tools;
mod ui;
mod widgets;

use eframe::egui;

fn options(renderer: eframe::Renderer) -> eframe::NativeOptions {
    eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default()
            .with_title("WinForge")
            .with_inner_size([1200.0, 780.0])
            .with_min_inner_size([1000.0, 640.0])
            .with_icon(std::sync::Arc::new(egui::IconData {
                rgba: widgets::ICON_128.to_vec(),
                width: 128,
                height: 128,
            })),
        ..Default::default()
    }
}

fn main() {
    install_panic_hook();

    // WinForge часто запускают сразу после установки Windows, когда драйвера
    // видеокарты ещё нет (или в виртуалке без 3D). OpenGL там только 1.1,
    // а DirectX 12 есть всегда — хотя бы программный (WARP). Поэтому сначала
    // wgpu (DirectX 12), а OpenGL — запасной вариант.
    let mut errors = Vec::new();
    for renderer in [eframe::Renderer::Wgpu, eframe::Renderer::Glow] {
        let result = eframe::run_native(
            "WinForge",
            options(renderer),
            Box::new(|cc| {
                theme::install(&cc.egui_ctx);
                Ok(Box::new(app::WinForgeApp::new()))
            }),
        );
        match result {
            Ok(()) => return,
            Err(e) => {
                let line = format!("{renderer:?}: {e}");
                log_error(&line);
                errors.push(line);
            }
        }
    }
    fatal(&format!(
        "WinForge не зміг створити вікно.\nWinForge could not create its window.\n\n{}",
        errors.join("\n")
    ));
}

/// Паника в release-сборке не видна (нет консоли): пишем в журнал и показываем окно.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("panic: {info}");
        log_error(&msg);
        fatal(&format!("WinForge аварійно завершився.\nWinForge crashed.\n\n{msg}"));
    }));
}

fn log_path() -> std::path::PathBuf {
    sys::data_dir().join("error.log")
}

fn log_error(msg: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(sys::data_dir());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "[{secs}] {msg}");
    }
}

/// Окно с ошибкой (работает без графики egui).
#[cfg(windows)]
fn fatal(msg: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let text = format!("{msg}\n\n{}", log_path().display());
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let (text, caption) = (wide(&text), wide("WinForge"));
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

#[cfg(not(windows))]
fn fatal(msg: &str) {
    eprintln!("{msg}");
}
