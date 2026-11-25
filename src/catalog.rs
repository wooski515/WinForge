//! Каталоги программ и твиков. Интерфейс строится из них автоматически.
//!
//! Файлы берутся из папки `catalog/` рядом с exe (или в текущей папке),
//! поэтому их можно править без перекомпиляции. Если файла нет — используется
//! встроенная в exe копия.
//!
//! Тексты двуязычные: `name = { uk = "...", en = "..." }`.
//! Для одинаковых на обоих языках строк (названия брендов) можно писать просто
//! `name = "Discord"`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

use crate::i18n::Lang;

/// Текст на двух языках.
#[derive(Debug, Clone, Default)]
pub struct Loc {
    pub uk: String,
    pub en: String,
}

impl Loc {
    pub fn get(&self, lang: Lang) -> &str {
        match lang {
            Lang::Uk => &self.uk,
            Lang::En => &self.en,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.uk.is_empty() && self.en.is_empty()
    }

    /// Содержит ли текст подстроку (на любом из языков, без учёта регистра).
    pub fn contains_lower(&self, needle_lower: &str) -> bool {
        self.uk.to_lowercase().contains(needle_lower) || self.en.to_lowercase().contains(needle_lower)
    }
}

impl<'de> Deserialize<'de> for Loc {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Same(String),
            Both { uk: String, en: String },
        }
        Ok(match Raw::deserialize(deserializer)? {
            Raw::Same(text) => Loc {
                uk: text.clone(),
                en: text,
            },
            Raw::Both { uk, en } => Loc { uk, en },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Safe,
    Moderate,
    Danger,
}

// ---------- Действия твика ----------

/// Где действие имеет смысл: на живой системе, в офлайн-образе или всегда.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Both,
    Live,
    Image,
}

/// Вид действия (определяется по заполненным полям).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Reg,
    Service,
    Task,
    Appx,
    Feature,
    /// Возможность Windows (Remove-WindowsCapability): старые компоненты вроде WordPad.
    Capability,
    Run,
    Unknown,
}

/// Одно конкретное изменение, которое выполняет твик.
///
/// В TOML — инлайн-таблица в массиве `actions`. Поля необязательные; вид
/// действия выводится из того, какие из них заполнены (см. `kind`). Строки с
/// обратными слешами пишутся литералами TOML: `reg = 'HKLM\...'`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Action {
    // --- реестр ---
    #[serde(default)]
    pub reg: Option<String>,
    /// Имя значения; `""` — значение по умолчанию (Default) ключа.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub dword: Option<i64>,
    #[serde(default)]
    pub qword: Option<i64>,
    #[serde(default)]
    pub sz: Option<String>,
    #[serde(default)]
    pub expand: Option<String>,
    #[serde(default)]
    pub multi: Option<Vec<String>>,
    /// REG_BINARY: байты в шестнадцатеричном виде, например "02 00 00 00".
    #[serde(default)]
    pub binary: Option<String>,
    // --- служба / задача / пакет / компонент ---
    #[serde(default)]
    pub service: Option<String>,
    /// Тип запуска службы: auto | delayed | manual | disabled.
    #[serde(default)]
    pub start: Option<String>,
    pub task: Option<String>,
    #[serde(default)]
    pub appx: Option<String>,
    #[serde(default)]
    pub feature: Option<String>,
    /// Возможность Windows по началу имени (без `~~~~версия`), например
    /// `Microsoft.Windows.WordPad`. Удаляется со всеми версиями и языками.
    #[serde(default)]
    pub capability: Option<String>,
    /// Для task / feature: включить (true) или выключить (false).
    #[serde(default)]
    pub enabled: Option<bool>,
    // --- произвольная команда ---
    #[serde(default)]
    pub run: Option<String>,
    /// Команда для отката действия `run` (если откат возможен).
    #[serde(default)]
    pub undo: Option<String>,
    /// Оболочка для `run`: cmd (по умолчанию) | powershell.
    #[serde(default)]
    pub shell: Option<String>,
    /// Для `run`: проверка в той же оболочке; если она печатает `True`, действие уже
    /// применено — команда не запускается и откат не записывается.
    #[serde(default)]
    pub check: Option<String>,
    // --- общее ---
    /// Удалить значение (если задано `name`) или весь ключ (для реестра).
    #[serde(default)]
    pub delete: bool,
    /// Ограничение области: "live" | "image". По умолчанию — всегда.
    #[serde(default)]
    pub when: Option<String>,
}

impl Action {
    pub fn kind(&self) -> ActionKind {
        if self.reg.is_some() {
            ActionKind::Reg
        } else if self.service.is_some() {
            ActionKind::Service
        } else if self.task.is_some() {
            ActionKind::Task
        } else if self.appx.is_some() {
            ActionKind::Appx
        } else if self.feature.is_some() {
            ActionKind::Feature
        } else if self.capability.is_some() {
            ActionKind::Capability
        } else if self.run.is_some() {
            ActionKind::Run
        } else {
            ActionKind::Unknown
        }
    }

    pub fn when(&self) -> When {
        match self.when.as_deref() {
            Some("live") => When::Live,
            Some("image") => When::Image,
            _ => When::Both,
        }
    }

    /// Техническая строка для подсказки (одинакова на всех языках).
    pub fn human(&self) -> String {
        match self.kind() {
            ActionKind::Reg => {
                let reg = self.reg.as_deref().unwrap_or("");
                match (&self.name, self.delete) {
                    (Some(n), true) if n.is_empty() => format!("{reg}\\(Default) → видалити"),
                    (Some(n), true) => format!("{reg}\\{n} → видалити"),
                    (None, true) => format!("{reg} → видалити ключ"),
                    (name, false) => {
                        let n = match name.as_deref() {
                            Some("") | None => "(Default)",
                            Some(n) => n,
                        };
                        format!("{reg}\\{n} = {}", self.reg_value_text())
                    }
                }
            }
            ActionKind::Service => format!(
                "Служба {} → {}",
                self.service.as_deref().unwrap_or(""),
                self.start.as_deref().unwrap_or("disabled")
            ),
            ActionKind::Task => format!(
                "Завдання {} → {}",
                self.task.as_deref().unwrap_or(""),
                if self.enabled == Some(true) { "увімкнено" } else { "вимкнено" }
            ),
            ActionKind::Appx => format!("Пакет {} → видалити", self.appx.as_deref().unwrap_or("")),
            ActionKind::Feature => format!(
                "Компонент {} → {}",
                self.feature.as_deref().unwrap_or(""),
                if self.enabled == Some(true) { "увімкнено" } else { "вимкнено" }
            ),
            ActionKind::Capability => {
                format!("Можливість {} → видалити", self.capability.as_deref().unwrap_or(""))
            }
            ActionKind::Run => format!("Команда: {}", self.run.as_deref().unwrap_or("")),
            ActionKind::Unknown => "невідома дія".into(),
        }
    }

    /// Текст значения реестра для подсказки.
    pub fn reg_value_text(&self) -> String {
        if let Some(v) = self.dword {
            format!("{v} (DWORD)")
        } else if let Some(v) = self.qword {
            format!("{v} (QWORD)")
        } else if let Some(v) = &self.sz {
            format!("\"{v}\"")
        } else if let Some(v) = &self.expand {
            format!("\"{v}\" (EXPAND)")
        } else if let Some(v) = &self.multi {
            format!("[{}]", v.join("; "))
        } else if let Some(v) = &self.binary {
            format!("{v} (BINARY)")
        } else {
            String::new()
        }
    }
}

// ---------- Программы ----------

#[derive(Debug, Deserialize)]
pub struct AppCatalog {
    pub category: Vec<AppCategory>,
}

#[derive(Debug, Deserialize)]
pub struct AppCategory {
    pub name: Loc,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub app: Vec<AppEntry>,
}

#[derive(Debug, Deserialize)]
pub struct AppEntry {
    pub id: String,
    pub name: Loc,
    pub winget: String,
    #[serde(default)]
    pub description: Loc,
    #[serde(default)]
    pub presets: Vec<String>,
}

// ---------- Твики (приватность, система) ----------

#[derive(Debug, Deserialize)]
pub struct TweakCatalog {
    pub group: Vec<TweakGroup>,
}

#[derive(Debug, Deserialize)]
pub struct TweakGroup {
    pub name: Loc,
    #[serde(default)]
    pub description: Loc,
    #[serde(default)]
    pub icon: String,
    /// В каталогах очистки, реестра и интегратора пункты называются `[[group.item]]`.
    #[serde(default, alias = "item")]
    pub tweak: Vec<Tweak>,
    /// Готовые наборы пунктов группы (например, режимы Windows Update):
    /// кнопка выбирает ровно эти пункты группы и снимает остальные.
    #[serde(default)]
    pub preset: Vec<GroupPreset>,
}

#[derive(Debug, Deserialize)]
pub struct GroupPreset {
    pub name: Loc,
    #[serde(default)]
    pub description: Loc,
    /// id пунктов этой же группы.
    #[serde(default)]
    pub tweaks: Vec<String>,
}

/// Особенность этого ПК, при которой пункт лучше не включать.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HwTrait {
    /// Есть батарея: ноутбук или планшет.
    Laptop,
    /// Система на жёстком диске.
    Hdd,
    /// Система на SSD.
    Ssd,
}

#[derive(Debug, Deserialize)]
pub struct Tweak {
    pub id: String,
    pub name: Loc,
    #[serde(default)]
    pub description: Loc,
    pub risk: Risk,
    #[serde(default)]
    pub recommended: bool,
    /// Технические подробности (ключи реестра, службы) — одинаковы для всех языков.
    /// Если пусто, генерируются из `actions`.
    #[serde(default)]
    pub details: Vec<String>,
    #[serde(default)]
    pub warning: Option<Loc>,
    /// Реальные действия твика (реестр, службы, задачи, компоненты, команды).
    #[serde(default)]
    pub actions: Vec<Action>,
    /// На каком оборудовании пункт не советуется (например, «Максимальна
    /// продуктивність» на ноутбуке). В режиме «Цей Windows» такие пункты
    /// помечаются и не включаются кнопками «Рекомендовані».
    #[serde(default)]
    pub avoid: Vec<HwTrait>,
    /// Для очистки: какие пути удалять (поддержка %ENV% и `*`).
    #[serde(default)]
    pub paths: Vec<String>,
    /// Для очистки: очистить корзину.
    #[serde(default)]
    pub recycle: bool,
    /// Для очистки: остановить эти службы перед удалением и запустить после.
    #[serde(default)]
    pub stop_services: Vec<String>,
}

impl Tweak {
    /// Строки подробностей: заданные вручную или сгенерированные из действий/путей.
    fn generated_details(&self) -> Vec<String> {
        if !self.details.is_empty() {
            return self.details.clone();
        }
        let mut out: Vec<String> = self.actions.iter().map(|a| a.human()).collect();
        out.extend(self.paths.iter().map(|p| format!("Очистити: {p}")));
        if self.recycle {
            out.push("Очистити кошик".into());
        }
        out
    }

}

// ---------- Встроенные приложения для удаления ----------

#[derive(Debug, Deserialize)]
pub struct BloatCatalog {
    /// Разделы страницы «Удаление» в порядке показа.
    #[serde(default)]
    pub group: Vec<BloatGroup>,
    pub app: Vec<BloatEntry>,
}

#[derive(Debug, Deserialize)]
pub struct BloatGroup {
    pub id: String,
    pub name: Loc,
    #[serde(default)]
    pub description: Loc,
    #[serde(default)]
    pub icon: String,
}

#[derive(Debug, Deserialize)]
pub struct BloatEntry {
    pub id: String,
    pub name: Loc,
    pub package: Loc,
    pub risk: Risk,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub warning: Option<Loc>,
    /// Раздел страницы (id из `[[group]]`).
    #[serde(default)]
    pub group: String,
    /// Действия удаления (обычно `appx`, иногда `run` / реестр).
    #[serde(default)]
    pub actions: Vec<Action>,
}

// ---------- Загрузка ----------

pub struct Catalogs {
    pub apps: AppCatalog,
    pub privacy: TweakCatalog,
    pub system: TweakCatalog,
    pub bloat: BloatCatalog,
    pub cleaner: TweakCatalog,
    pub registry: TweakCatalog,
    pub integrator: TweakCatalog,
}

impl Catalogs {
    /// Заполняет пустые `details` строками, сгенерированными из действий/путей.
    fn fill_details(&mut self) {
        let cats = [
            &mut self.privacy,
            &mut self.system,
            &mut self.cleaner,
            &mut self.registry,
            &mut self.integrator,
        ];
        for cat in cats {
            for group in &mut cat.group {
                for tweak in &mut group.tweak {
                    if tweak.details.is_empty() {
                        tweak.details = tweak.generated_details();
                    }
                }
            }
        }
    }

    /// Все каталоги с переключателями (для поиска пункта по id).
    pub fn tweak_catalogs(&self) -> [&TweakCatalog; 5] {
        [
            &self.privacy,
            &self.system,
            &self.cleaner,
            &self.registry,
            &self.integrator,
        ]
    }
}

const EMBEDDED_APPS: &str = include_str!("../catalog/apps.toml");
const EMBEDDED_PRIVACY: &str = include_str!("../catalog/privacy.toml");
const EMBEDDED_SYSTEM: &str = include_str!("../catalog/system.toml");
const EMBEDDED_BLOAT: &str = include_str!("../catalog/bloat.toml");
const EMBEDDED_CLEANER: &str = include_str!("../catalog/cleaner.toml");
const EMBEDDED_REGISTRY: &str = include_str!("../catalog/registry.toml");
const EMBEDDED_INTEGRATOR: &str = include_str!("../catalog/integrator.toml");

impl Catalogs {
    /// Загружает каталоги. Возвращает также список ошибок разбора файлов с диска
    /// (в этом случае для такого файла используется встроенная копия).
    pub fn load() -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let mut catalogs = Self {
            apps: load_one("apps.toml", EMBEDDED_APPS, &mut errors),
            privacy: load_one("privacy.toml", EMBEDDED_PRIVACY, &mut errors),
            system: load_one("system.toml", EMBEDDED_SYSTEM, &mut errors),
            bloat: load_one("bloat.toml", EMBEDDED_BLOAT, &mut errors),
            cleaner: load_one("cleaner.toml", EMBEDDED_CLEANER, &mut errors),
            registry: load_one("registry.toml", EMBEDDED_REGISTRY, &mut errors),
            integrator: load_one("integrator.toml", EMBEDDED_INTEGRATOR, &mut errors),
        };
        catalogs.fill_details();
        (catalogs, errors)
    }
}

fn read_from_disk(name: &str) -> Option<String> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("catalog").join(name));
        }
    }
    candidates.push(std::path::PathBuf::from("catalog").join(name));
    candidates
        .into_iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
}

fn load_one<T: DeserializeOwned>(name: &str, embedded: &str, errors: &mut Vec<String>) -> T {
    if let Some(text) = read_from_disk(name) {
        match toml::from_str(&text) {
            Ok(value) => return value,
            Err(e) => errors.push(format!("catalog/{name}: {e}")),
        }
    }
    toml::from_str(embedded).unwrap_or_else(|e| panic!("встроенный каталог {name} повреждён: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn embedded() -> Catalogs {
        let mut c = Catalogs {
            apps: toml::from_str(EMBEDDED_APPS).expect("apps.toml"),
            privacy: toml::from_str(EMBEDDED_PRIVACY).expect("privacy.toml"),
            system: toml::from_str(EMBEDDED_SYSTEM).expect("system.toml"),
            bloat: toml::from_str(EMBEDDED_BLOAT).expect("bloat.toml"),
            cleaner: toml::from_str(EMBEDDED_CLEANER).expect("cleaner.toml"),
            registry: toml::from_str(EMBEDDED_REGISTRY).expect("registry.toml"),
            integrator: toml::from_str(EMBEDDED_INTEGRATOR).expect("integrator.toml"),
        };
        c.fill_details();
        c
    }

    /// Все двуязычные тексты каталогов вместе с подписью, откуда они.
    fn all_texts(c: &Catalogs) -> Vec<(String, &Loc)> {
        let mut out: Vec<(String, &Loc)> = Vec::new();
        for cat in &c.apps.category {
            out.push(("category".into(), &cat.name));
            for a in &cat.app {
                out.push((a.id.clone(), &a.name));
                out.push((a.id.clone(), &a.description));
            }
        }
        for g in c.tweak_catalogs().into_iter().flat_map(|cat| cat.group.iter()) {
            out.push(("group".into(), &g.name));
            out.push(("group".into(), &g.description));
            for p in &g.preset {
                out.push(("preset".into(), &p.name));
                out.push(("preset".into(), &p.description));
            }
            for t in &g.tweak {
                out.push((t.id.clone(), &t.name));
                out.push((t.id.clone(), &t.description));
                if let Some(w) = &t.warning {
                    out.push((t.id.clone(), w));
                }
            }
        }
        for g in &c.bloat.group {
            out.push((g.id.clone(), &g.name));
            out.push((g.id.clone(), &g.description));
        }
        for a in &c.bloat.app {
            out.push((a.id.clone(), &a.name));
            out.push((a.id.clone(), &a.package));
            if let Some(w) = &a.warning {
                out.push((a.id.clone(), w));
            }
        }
        out
    }

    #[test]
    fn embedded_catalogs_parse() {
        let c = embedded();
        assert!(!c.apps.category.is_empty());
        assert!(!c.privacy.group.is_empty());
        assert!(!c.system.group.is_empty());
        assert!(!c.bloat.app.is_empty());
        for cat in c.tweak_catalogs() {
            assert!(!cat.group.is_empty());
            for g in &cat.group {
                assert!(!g.tweak.is_empty(), "пустая группа {}", g.name.en);
            }
        }
    }

    #[test]
    fn ids_are_unique() {
        let c = embedded();
        let mut seen = HashSet::new();
        let ids = c
            .apps
            .category
            .iter()
            .flat_map(|cat| cat.app.iter().map(|a| a.id.as_str()))
            .chain(
                c.tweak_catalogs()
                    .into_iter()
                    .flat_map(|cat| cat.group.iter())
                    .flat_map(|g| g.tweak.iter().map(|t| t.id.as_str())),
            )
            .chain(c.bloat.app.iter().map(|a| a.id.as_str()));
        for id in ids {
            assert!(seen.insert(id), "повторяющийся id: {id}");
        }
    }

    #[test]
    fn danger_items_have_warning() {
        let c = embedded();
        for g in c.tweak_catalogs().into_iter().flat_map(|cat| cat.group.iter()) {
            for t in &g.tweak {
                if t.risk == Risk::Danger {
                    assert!(t.warning.is_some(), "у опасного твика {} нет warning", t.id);
                }
            }
        }
        for a in &c.bloat.app {
            if a.risk == Risk::Danger {
                assert!(a.warning.is_some(), "у опасного пункта {} нет warning", a.id);
            }
        }
    }

    #[test]
    fn english_texts_have_no_cyrillic() {
        let c = embedded();
        for (owner, text) in all_texts(&c) {
            assert!(
                !text.en.chars().any(|ch| ('\u{0400}'..='\u{04FF}').contains(&ch)),
                "{owner}: в английском тексте кириллица: {}",
                text.en
            );
        }
    }

    #[test]
    fn ukrainian_texts_have_no_russian_letters() {
        // В украинском нет букв ы, э, ъ, ё — их появление значит, что остался русский текст.
        let c = embedded();
        for (owner, text) in all_texts(&c) {
            let lower = text.uk.to_lowercase();
            assert!(
                !lower.chars().any(|ch| matches!(ch, 'ы' | 'э' | 'ъ' | 'ё')),
                "{owner}: в украинском тексте русские буквы: {}",
                text.uk
            );
        }
    }

    #[test]
    fn no_yandex() {
        let c = embedded();
        for cat in &c.apps.category {
            for a in &cat.app {
                assert!(!a.winget.to_lowercase().contains("yandex"), "{}", a.id);
            }
        }
    }

    /// Каждое заданное действие должно опознаваться (не Unknown).
    #[test]
    fn actions_resolve_to_known_kind() {
        let c = embedded();
        for cat in c.tweak_catalogs() {
            for g in &cat.group {
                for t in &g.tweak {
                    for a in &t.actions {
                        assert_ne!(a.kind(), ActionKind::Unknown, "{}: неизвестное действие {a:?}", t.id);
                    }
                }
            }
        }
        for a in &c.bloat.app {
            for act in &a.actions {
                assert_ne!(act.kind(), ActionKind::Unknown, "{}: неизвестное действие", a.id);
            }
        }
    }

    /// У реестрового действия ровно один способ задать значение (или удаление).
    #[test]
    fn reg_actions_have_one_value() {
        let c = embedded();
        for cat in c.tweak_catalogs() {
            for g in &cat.group {
                for t in &g.tweak {
                    for a in &t.actions {
                        if a.kind() != ActionKind::Reg {
                            continue;
                        }
                        let values = [
                            a.dword.is_some(),
                            a.qword.is_some(),
                            a.sz.is_some(),
                            a.expand.is_some(),
                            a.multi.is_some(),
                            a.binary.is_some(),
                        ]
                        .iter()
                        .filter(|b| **b)
                        .count();
                        if a.delete {
                            assert_eq!(values, 0, "{}: delete и значение вместе", t.id);
                        } else {
                            assert_eq!(values, 1, "{}: у {:?} должно быть ровно одно значение", t.id, a.reg);
                        }
                    }
                }
            }
        }
    }

    /// Каждый пункт «Удаления» лежит в существующем разделе, и разделы не пустые.
    #[test]
    fn bloat_groups_are_consistent() {
        let c = embedded();
        let ids: HashSet<&str> = c.bloat.group.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids.len(), c.bloat.group.len(), "повторяющийся раздел");
        for a in &c.bloat.app {
            assert!(ids.contains(a.group.as_str()), "{}: нет раздела {:?}", a.id, a.group);
            assert!(!a.actions.is_empty(), "{}: нет действий", a.id);
        }
        for g in &c.bloat.group {
            assert!(c.bloat.app.iter().any(|a| a.group == g.id), "пустой раздел {}", g.id);
        }
    }

    /// Пакеты и возможности не дублируются между пунктами «Удаления».
    #[test]
    fn bloat_packages_are_unique() {
        let c = embedded();
        let mut seen = HashSet::new();
        for a in &c.bloat.app {
            for act in &a.actions {
                if let Some(p) = act.appx.as_deref().or(act.capability.as_deref()) {
                    assert!(seen.insert(p.to_lowercase()), "{}: пакет {p} уже есть в другом пункте", a.id);
                }
            }
        }
    }

    /// Наборы ссылаются только на пункты своей группы.
    #[test]
    fn group_presets_point_into_their_group() {
        let c = embedded();
        let mut presets = 0;
        for g in c.tweak_catalogs().into_iter().flat_map(|cat| cat.group.iter()) {
            for p in &g.preset {
                presets += 1;
                for id in &p.tweaks {
                    assert!(g.tweak.iter().any(|t| &t.id == id), "{}: набора {:?} нет в группе", id, p.name.en);
                }
            }
        }
        assert!(presets >= 4, "режимы Windows Update пропали");
    }

    /// Два пункта не пишут в одно и то же значение реестра разные данные:
    /// иначе результат зависит от порядка, а откат одного ломает другой.
    #[test]
    fn no_two_tweaks_write_one_value_differently() {
        let c = embedded();
        let mut seen: std::collections::HashMap<String, (String, String)> = Default::default();
        for g in c.tweak_catalogs().into_iter().flat_map(|cat| cat.group.iter()) {
            for t in &g.tweak {
                for a in t.actions.iter().filter(|a| a.kind() == ActionKind::Reg) {
                    let Some(name) = &a.name else { continue };
                    let key = format!("{}\\{}", a.reg.as_deref().unwrap_or("").to_lowercase(), name.to_lowercase());
                    let value = if a.delete { "<delete>".to_string() } else { a.reg_value_text() };
                    if let Some((other, v)) = seen.get(&key) {
                        assert!(v == &value || other == &t.id, "{key}: {other} пише {v}, а {} — {value}", t.id);
                    } else {
                        seen.insert(key, (t.id.clone(), value));
                    }
                }
            }
        }
    }

    /// Каждый пункт очистки действительно что-то чистит.
    #[test]
    fn cleaner_items_clean_something() {
        let c = embedded();
        for g in &c.cleaner.group {
            for t in &g.tweak {
                assert!(
                    !t.paths.is_empty() || t.recycle,
                    "у пункта очистки {} нет ни paths, ни recycle",
                    t.id
                );
            }
        }
    }
}
