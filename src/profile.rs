//! Профиль: все отметки и настройки WinForge в одном .toml-файле, чтобы через
//! год (или на ПК друга) вернуть всё как было. Пароль в профиль не пишется.

use serde::{Deserialize, Serialize};

use crate::app::{WinForgeApp, EDITIONS, KEYBOARDS, LANGUAGES, TIMEZONES};
use crate::i18n::Lang;
use crate::tools::CustomApp;

pub const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    /// "uk" | "en"
    #[serde(default)]
    pub lang: String,
    /// Выбранные программы (id из каталога и свои `my-N`).
    #[serde(default)]
    pub apps: Vec<String>,
    /// Включённые твики и отмеченные пункты всех разделов.
    #[serde(default)]
    pub enabled: Vec<String>,
    #[serde(default)]
    pub image: ImageProfile,
    #[serde(default)]
    pub install: InstallProfile,
    /// Команды Win+R: [псевдоним, что открывать].
    #[serde(default)]
    pub run_commands: Vec<(String, String)>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageProfile {
    /// Название редакции на английском, например "Windows 11 Pro".
    pub edition: String,
    pub language: usize,
    pub keyboard: usize,
    pub timezone: usize,
    pub username: String,
    pub computer_name: String,
    pub local_account: bool,
    pub auto_login: bool,
    pub skip_oobe: bool,
    pub bypass_tpm: bool,
    pub no_network: bool,
    pub cleanup: bool,
    pub drivers_path: String,
    /// Добавить в образ драйверы этого ПК.
    pub export_drivers: bool,
    /// Сеть Wi-Fi (пароль в профиль не пишется).
    pub wifi_ssid: String,
    pub guest_tools: bool,
    pub compact_os: bool,
    /// KMS-сервер организации (ключ продукта в профиль не пишется).
    pub kms_host: String,
    pub output_path: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InstallProfile {
    pub folder: String,
    pub download_folder: String,
    pub delete_installers: bool,
    pub custom: Vec<CustomApp>,
}

impl WinForgeApp {
    pub fn to_profile(&self) -> Profile {
        let img = &self.image;
        let mut apps: Vec<String> = self.selected_apps.iter().cloned().collect();
        apps.sort();
        let mut enabled: Vec<String> = self.enabled.iter().cloned().collect();
        enabled.sort();
        Profile {
            version: VERSION,
            lang: match self.lang {
                Lang::Uk => "uk".into(),
                Lang::En => "en".into(),
            },
            apps,
            enabled,
            image: ImageProfile {
                edition: EDITIONS[img.edition].1.into(),
                language: img.language,
                keyboard: img.keyboard,
                timezone: img.timezone,
                username: img.username.clone(),
                computer_name: img.computer_name.clone(),
                local_account: img.local_account,
                auto_login: img.auto_login,
                skip_oobe: img.skip_oobe,
                bypass_tpm: img.bypass_tpm,
                no_network: img.no_network,
                cleanup: img.cleanup,
                drivers_path: img.drivers_path.clone(),
                export_drivers: img.export_drivers,
                wifi_ssid: img.wifi_ssid.clone(),
                guest_tools: img.guest_tools,
                compact_os: img.compact_os,
                kms_host: img.kms_host.clone(),
                output_path: img.output_path.clone(),
            },
            install: InstallProfile {
                folder: self.install.folder.clone(),
                download_folder: self.install.download_folder.clone(),
                delete_installers: self.install.delete_installers,
                custom: self.install.custom.clone(),
            },
            run_commands: self.tools.run_commands.clone(),
        }
    }

    /// Применяет профиль. Неизвестные id (из старых каталогов) пропускаются.
    pub fn apply_profile(&mut self, p: Profile) {
        self.lang = if p.lang == "en" { Lang::En } else { Lang::Uk };

        let known_ids: std::collections::HashSet<String> = self
            .cat
            .tweak_catalogs()
            .into_iter()
            .flat_map(|c| c.group.iter())
            .flat_map(|g| g.tweak.iter().map(|t| t.id.clone()))
            .chain(self.cat.bloat.app.iter().map(|a| a.id.clone()))
            .collect();
        self.enabled = p.enabled.into_iter().filter(|id| known_ids.contains(id)).collect();

        self.install.custom = p.install.custom;
        let custom_ids: std::collections::HashSet<String> =
            self.install.custom.iter().map(|c| c.id.clone()).collect();
        let catalog_apps: std::collections::HashSet<String> = self
            .cat
            .apps
            .category
            .iter()
            .flat_map(|c| c.app.iter().map(|a| a.id.clone()))
            .collect();
        self.selected_apps = p
            .apps
            .into_iter()
            .filter(|id| catalog_apps.contains(id) || custom_ids.contains(id))
            .collect();
        // Следующий id своей программы — после самого большого из профиля.
        self.install.next_custom_id = self
            .install
            .custom
            .iter()
            .filter_map(|c| c.id.strip_prefix("my-").and_then(|n| n.parse::<u32>().ok()))
            .max()
            .unwrap_or(0)
            + 1;
        if !p.install.folder.is_empty() {
            self.install.folder = p.install.folder;
        }
        if !p.install.download_folder.is_empty() {
            self.install.download_folder = p.install.download_folder;
        }
        self.install.delete_installers = p.install.delete_installers;

        let img = &mut self.image;
        let i = p.image;
        if let Some(idx) = EDITIONS.iter().position(|e| e.1 == i.edition) {
            img.edition = idx;
        }
        img.language = i.language.min(LANGUAGES.len() - 1);
        img.keyboard = i.keyboard.min(KEYBOARDS.len() - 1);
        img.timezone = i.timezone.min(TIMEZONES.len() - 1);
        if !i.username.is_empty() {
            img.username = i.username;
        }
        if !i.computer_name.is_empty() {
            img.computer_name = i.computer_name;
        }
        img.local_account = i.local_account;
        img.auto_login = i.auto_login;
        img.skip_oobe = i.skip_oobe;
        img.bypass_tpm = i.bypass_tpm;
        img.no_network = i.no_network;
        img.cleanup = i.cleanup;
        img.drivers_path = i.drivers_path;
        img.export_drivers = i.export_drivers;
        img.wifi_ssid = i.wifi_ssid;
        img.guest_tools = i.guest_tools;
        img.compact_os = i.compact_os;
        img.kms_host = i.kms_host;
        if !i.output_path.is_empty() {
            img.output_path = i.output_path;
        }

        self.tools.run_commands = p.run_commands;
    }

    /// Папка профилей по умолчанию.
    fn profiles_dir() -> std::path::PathBuf {
        let dir = crate::sys::data_dir().join("profiles");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// Диалог «Сохранить профиль». Возвращает текст уведомления (или None, если отменено).
    pub fn save_profile_dialog(&mut self) -> Option<String> {
        let l = self.lang;
        let name = match self.profile_name.trim() {
            "" => "profile".to_owned(),
            n => n.to_owned(),
        };
        let path = rfd::FileDialog::new()
            .set_title(l.tr("Зберегти профіль", "Save profile"))
            .set_directory(Self::profiles_dir())
            .set_file_name(format!("{name}.toml"))
            .add_filter(l.tr("Профіль WinForge", "WinForge profile"), &["toml"])
            .save_file()?;
        let text = match toml::to_string_pretty(&self.to_profile()) {
            Ok(t) => format!("# WinForge profile\n{t}"),
            Err(e) => return Some(format!("{}: {e}", l.tr("Помилка", "Error"))),
        };
        Some(match std::fs::write(&path, text) {
            Ok(()) => {
                if let Some(stem) = path.file_stem() {
                    self.profile_name = stem.to_string_lossy().into_owned();
                }
                match l {
                    Lang::Uk => format!("Профіль збережено: {}", path.display()),
                    Lang::En => format!("Profile saved: {}", path.display()),
                }
            }
            Err(e) => format!("{}: {e}", l.tr("Не вдалося зберегти", "Could not save")),
        })
    }

    /// Диалог «Открыть профиль». Возвращает текст уведомления (или None, если отменено).
    pub fn open_profile_dialog(&mut self) -> Option<String> {
        let l = self.lang;
        let path = rfd::FileDialog::new()
            .set_title(l.tr("Відкрити профіль", "Open profile"))
            .set_directory(Self::profiles_dir())
            .add_filter(l.tr("Профіль WinForge", "WinForge profile"), &["toml"])
            .pick_file()?;
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => return Some(format!("{}: {e}", l.tr("Не вдалося прочитати", "Could not read"))),
        };
        Some(match toml::from_str::<Profile>(&text) {
            Ok(profile) => {
                self.apply_profile(profile);
                if let Some(stem) = path.file_stem() {
                    self.profile_name = stem.to_string_lossy().into_owned();
                }
                let l = self.lang;
                l.tr("Профіль завантажено", "Profile loaded").into()
            }
            Err(e) => format!("{}: {e}", l.tr("Це не профіль WinForge", "Not a WinForge profile")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::CustomKind;

    #[test]
    fn profile_roundtrip() {
        let mut app = WinForgeApp::new();
        app.image.edition = 1;
        app.image.username = "Ivan".into();
        app.image.password = "secret".into();
        app.install.custom.push(CustomApp {
            id: "my-7".into(),
            name: "Tool".into(),
            kind: CustomKind::Winget,
            value: "Some.Tool".into(),
            args: String::new(),
        });
        app.selected_apps.insert("my-7".into());
        app.tools.run_commands = vec![("tm".into(), "taskmgr.exe".into())];
        app.image.wifi_ssid = "HomeNet".into();
        app.image.wifi_password = "wifi-pass-123".into();
        app.image.product_key = "W269N-WFGWX-YVC9B-4J6C9-T83GX".into();
        app.image.kms_host = "kms.company.local".into();
        app.image.compact_os = true;
        app.image.guest_tools = true;
        app.image.export_drivers = true;
        let enabled_before = app.enabled.clone();

        let text = toml::to_string_pretty(&app.to_profile()).unwrap();
        assert!(!text.contains("secret"), "пароль не должен попадать в профиль");
        assert!(!text.contains("wifi-pass-123"), "пароль Wi-Fi не должен попадать в профиль");
        assert!(!text.contains("W269N"), "ключ продукта не должен попадать в профиль");

        let mut other = WinForgeApp::new();
        other.enabled.clear();
        other.apply_profile(toml::from_str(&text).unwrap());
        assert_eq!(other.enabled, enabled_before);
        assert_eq!(other.image.edition, 1);
        assert_eq!(other.image.username, "Ivan");
        assert!(other.selected_apps.contains("my-7"));
        assert_eq!(other.install.next_custom_id, 8);
        assert_eq!(other.tools.run_commands.len(), 1);
        assert_eq!(other.image.wifi_ssid, "HomeNet");
        assert_eq!(other.image.kms_host, "kms.company.local");
        assert!(other.image.compact_os && other.image.guest_tools && other.image.export_drivers);
        assert!(other.image.wifi_password.is_empty() && other.image.product_key.is_empty());
    }

    /// Профиль от старой версии (без новых полей) открывается.
    #[test]
    fn old_profile_still_loads() {
        let old = "version = 1\nlang = \"uk\"\napps = []\nenabled = []\n[image]\nedition = \"Windows 11 Pro\"\nusername = \"User\"\n";
        let p: Profile = toml::from_str(old).expect("старый профиль");
        let mut app = WinForgeApp::new();
        app.apply_profile(p);
        assert!(app.image.kms_host.is_empty() && !app.image.compact_os && !app.image.export_drivers);
    }

    #[test]
    fn unknown_ids_are_dropped() {
        let mut app = WinForgeApp::new();
        let mut p = app.to_profile();
        p.enabled.push("no-such-tweak".into());
        p.apps.push("no-such-app".into());
        app.apply_profile(p);
        assert!(!app.enabled.contains("no-such-tweak"));
        assert!(!app.selected_apps.contains("no-such-app"));
    }
}
