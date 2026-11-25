//! Что WinForge применял к этой системе и на какой сборке Windows.
//!
//! Крупное обновление Windows (новая сборка) иногда возвращает рекламу,
//! телеметрию и удалённые приложения. Если сборка сменилась, WinForge
//! предлагает снова выбрать эти пункты и проверить, что вернулось.
//! Ничего не применяется само: только выбор пунктов и предпросмотр.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppliedState {
    /// Сборка Windows (CurrentBuild), на которой применяли последний раз.
    pub build: String,
    /// id применённых твиков и пунктов удаления (все применения вместе).
    pub ids: Vec<String>,
}

impl AppliedState {
    /// Добавляет новые id к уже записанным и запоминает текущую сборку.
    pub fn merge(&mut self, ids: impl IntoIterator<Item = String>, build: &str) {
        for id in ids {
            if !self.ids.contains(&id) {
                self.ids.push(id);
            }
        }
        self.build = build.to_owned();
    }

    /// Сменилась ли сборка Windows с последнего применения.
    pub fn build_changed(&self, current: &str) -> bool {
        !self.ids.is_empty() && !self.build.is_empty() && !current.is_empty() && self.build != current
    }
}

fn path() -> PathBuf {
    super::data_dir().join("applied.toml")
}

pub fn load() -> AppliedState {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(state: &AppliedState) -> std::io::Result<()> {
    std::fs::create_dir_all(super::data_dir())?;
    let text = toml::to_string_pretty(state).map_err(std::io::Error::other)?;
    std::fs::write(path(), text)
}

/// Номер сборки Windows, например «26100». Пусто, если не прочитать.
pub fn windows_build() -> String {
    let Some(raw) = super::reg::read_raw(
        super::reg::Root::Hklm,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "CurrentBuild",
    ) else {
        return String::new();
    };
    let wide: Vec<u16> = raw
        .bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&wide).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_old_ids_and_takes_new_build() {
        let mut s = AppliedState::default();
        s.merge(["a".to_string(), "b".to_string()], "26100");
        s.merge(["b".to_string(), "c".to_string()], "26200");
        assert_eq!(s.ids, ["a", "b", "c"]);
        assert_eq!(s.build, "26200");
    }

    #[test]
    fn only_a_new_build_with_history_counts() {
        let mut s = AppliedState::default();
        assert!(!s.build_changed("26200"), "ничего не применяли — не напоминаем");
        s.merge(["a".to_string()], "26100");
        assert!(!s.build_changed("26100"));
        assert!(s.build_changed("26200"));
        assert!(!s.build_changed(""), "сборку не прочитали — молчим");
        let back: AppliedState = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
