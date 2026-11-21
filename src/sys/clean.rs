//! Очистка: раскрытие путей (с поддержкой %ENV% и `*`), подсчёт размера,
//! удаление содержимого. Заблокированные файлы пропускаются.

use std::path::{Path, PathBuf};

use crate::sys::engine::os;
use crate::sys::probe::expand;

/// Простое сопоставление с шаблоном из одного `*` (или без него).
fn wildcard_match(pattern: &str, name: &str) -> bool {
    let p = pattern.to_lowercase();
    let n = name.to_lowercase();
    match p.split_once('*') {
        None => p == n,
        Some((head, tail)) => n.len() >= head.len() + tail.len() && n.starts_with(head) && n.ends_with(tail),
    }
}

/// Раскрывает один путь (с env и `*`) в список существующих путей.
pub fn expand_one(pattern: &str) -> Vec<PathBuf> {
    let expanded = expand(pattern);
    let norm = expanded.replace('/', "\\");
    let comps: Vec<&str> = norm.split('\\').filter(|c| !c.is_empty()).collect();
    if comps.is_empty() {
        return Vec::new();
    }
    // Первый компонент — диск (C:) или UNC; начинаем от него.
    let base = PathBuf::from(format!("{}\\", comps[0]));
    collect(&base, &comps[1..])
}

fn collect(base: &Path, comps: &[&str]) -> Vec<PathBuf> {
    if comps.is_empty() {
        return if base.exists() { vec![base.to_path_buf()] } else { Vec::new() };
    }
    let (head, tail) = (comps[0], &comps[1..]);
    if head.contains('*') {
        let Ok(entries) = std::fs::read_dir(base) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|e| wildcard_match(head, &e.file_name().to_string_lossy()))
            .flat_map(|e| collect(&e.path(), tail))
            .collect()
    } else {
        collect(&base.join(head), tail)
    }
}

/// Размер пути в байтах (файл или всё содержимое папки).
pub fn size_of(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    if meta.file_type().is_symlink() {
        return 0;
    }
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.flatten() {
            total += size_of(&e.path());
        }
    }
    total
}

/// Суммарный размер всех путей твика (в байтах).
pub fn measure(patterns: &[String]) -> u64 {
    patterns
        .iter()
        .flat_map(|p| expand_one(p))
        .map(|p| size_of(&p))
        .sum()
}

/// Удаляет содержимое пути. Для папки удаляет её содержимое, но не саму папку.
/// Возвращает освобождённые байты.
fn delete_path(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() || meta.file_type().is_symlink() {
        let size = meta.len();
        return if std::fs::remove_file(path).is_ok() { size } else { 0 };
    }
    // Папка: удаляем содержимое, саму папку оставляем.
    let mut freed = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.flatten() {
            let p = e.path();
            let size = size_of(&p);
            let ok = if p.is_dir() {
                std::fs::remove_dir_all(&p).is_ok()
            } else {
                std::fs::remove_file(&p).is_ok()
            };
            if ok {
                freed += size;
            }
        }
    }
    freed
}

/// Выполняет очистку одного пункта. Возвращает освобождённые байты.
pub fn clean(patterns: &[String], recycle: bool, stop_services: &[String]) -> u64 {
    for svc in stop_services {
        let _ = os::run("cmd", &format!("sc stop \"{svc}\""));
    }

    let mut freed = 0;
    for pattern in patterns {
        for path in expand_one(pattern) {
            freed += delete_path(&path);
        }
    }

    if recycle {
        let _ = os::run("powershell", "Clear-RecycleBin -Force -ErrorAction SilentlyContinue");
    }

    for svc in stop_services {
        let _ = os::run("cmd", &format!("sc start \"{svc}\""));
    }
    freed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard() {
        assert!(wildcard_match("Cache", "cache"));
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("thumbcache_*.db", "thumbcache_1024.db"));
        assert!(!wildcard_match("thumbcache_*.db", "iconcache.db"));
        assert!(wildcard_match("*.tmp", "a.tmp"));
    }
}
