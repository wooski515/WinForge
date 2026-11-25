//! Исполнитель действий каталога: применяет одно действие к живой системе,
//! проверяет его текущее состояние и записывает данные для отката.

use serde::{Deserialize, Serialize};

use crate::catalog::{Action, ActionKind, When};
use crate::sys::reg::{self, Root, ValueData};

/// Состояние действия на системе (для предпросмотра).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Уже применено.
    Applied,
    /// Ещё не применено — будет изменено.
    Pending,
    /// Проверить дёшево нельзя (задача, пакет, команда).
    Unknown,
    /// Неприменимо на этой системе (например, такой службы нет).
    NotApplicable,
}

/// Запись для отката одного изменения.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Undo {
    Reg {
        root: Root,
        subpath: String,
        name: String,
        prev: Option<reg::RawValue>,
    },
    /// Ключ создан нами — при откате удаляется целиком.
    KeyCreated {
        root: Root,
        subpath: String,
    },
    Task {
        task: String,
        enable: bool,
    },
    Feature {
        feature: String,
        enable: bool,
    },
    Run {
        cmd: String,
        shell: String,
    },
}

impl Undo {
    /// Что именно откатывается — для сообщений об ошибках.
    pub fn describe(&self, l: crate::i18n::Lang) -> String {
        match self {
            Undo::Reg { root, subpath, name, .. } => {
                let name = if name.is_empty() { "(Default)" } else { name.as_str() };
                format!("{root:?}\\{subpath}\\{name}")
            }
            Undo::KeyCreated { root, subpath } => format!("{root:?}\\{subpath}"),
            Undo::Task { task, .. } => format!("{} {task}", l.tr("Завдання", "Task")),
            Undo::Feature { feature, .. } => format!("{} {feature}", l.tr("Компонент", "Feature")),
            Undo::Run { cmd, .. } if cmd.contains("Add-WindowsCapability") => {
                // Имена возможностей стоят в кавычках: 'Media.WindowsMediaPlayer~~~~0.0.12.0'.
                let names: Vec<&str> = cmd
                    .split('\'')
                    .skip(1)
                    .step_by(2)
                    .map(|n| n.split('~').next().unwrap_or(n))
                    .collect();
                format!(
                    "{} {} — {}",
                    l.tr("Компонент", "Feature"),
                    names.join(", "),
                    l.tr("завантаження з Windows Update, кілька хвилин", "downloading from Windows Update, a few minutes")
                )
            }
            Undo::Run { cmd, .. } => {
                let short: String = cmd.chars().take(80).collect();
                format!("{} {short}", l.tr("Команда", "Command"))
            }
        }
    }
}

/// Относится ли действие к живой системе.
pub fn applies_live(a: &Action) -> bool {
    matches!(a.when(), When::Both | When::Live)
}

/// Относится ли действие к офлайн-образу.
pub fn applies_image(a: &Action) -> bool {
    matches!(a.when(), When::Both | When::Image)
}

/// Значение реестра из полей действия.
fn value_of(a: &Action) -> Option<ValueData> {
    if let Some(v) = a.dword {
        Some(ValueData::Dword(v as u32))
    } else if let Some(v) = a.qword {
        Some(ValueData::Qword(v as u64))
    } else if let Some(s) = &a.sz {
        Some(ValueData::Sz(s.clone()))
    } else if let Some(s) = &a.expand {
        Some(ValueData::Expand(s.clone()))
    } else if let Some(list) = &a.multi {
        Some(ValueData::Multi(list.clone()))
    } else {
        a.binary.as_ref().map(|h| ValueData::Binary(reg::parse_hex(h)))
    }
}

/// start = auto|delayed|manual|disabled → (Start, DelayedAutostart).
fn service_start(start: &str) -> (u32, u32) {
    match start.to_lowercase().as_str() {
        "auto" | "automatic" => (2, 0),
        "delayed" | "delayed-auto" => (2, 1),
        "manual" | "demand" => (3, 0),
        "boot" => (0, 0),
        "system" => (1, 0),
        _ => (4, 0), // disabled
    }
}

/// Текущее состояние действия (быстрые проверки — только реестр и службы).
pub fn status(a: &Action) -> Status {
    match a.kind() {
        ActionKind::Reg => {
            let Some((root, sub)) = a.reg.as_deref().and_then(reg::split) else {
                return Status::Unknown;
            };
            let name = a.name.clone().unwrap_or_default();
            if a.delete {
                let gone = if a.name.is_some() {
                    reg::read_raw(root, &sub, &name).is_none()
                } else {
                    !reg::key_exists(root, &sub)
                };
                return if gone { Status::Applied } else { Status::Pending };
            }
            match (value_of(a), reg::read_raw(root, &sub, &name)) {
                (Some(target), Some(cur)) => {
                    let t = target.raw();
                    if t.vtype == cur.vtype && t.bytes == cur.bytes {
                        Status::Applied
                    } else {
                        Status::Pending
                    }
                }
                (Some(_), None) => Status::Pending,
                _ => Status::Unknown,
            }
        }
        ActionKind::Service => {
            let Some(svc) = &a.service else { return Status::Unknown };
            let (want, _) = service_start(a.start.as_deref().unwrap_or("disabled"));
            let sub = format!(r"SYSTEM\CurrentControlSet\Services\{svc}");
            if !reg::key_exists(Root::Hklm, &sub) {
                return Status::NotApplicable;
            }
            match reg::read_raw(Root::Hklm, &sub, "Start") {
                Some(raw) if raw.bytes.len() >= 4 => {
                    let cur = u32::from_le_bytes([raw.bytes[0], raw.bytes[1], raw.bytes[2], raw.bytes[3]]);
                    if cur == want {
                        Status::Applied
                    } else {
                        Status::Pending
                    }
                }
                _ => Status::Unknown,
            }
        }
        // Команда с проверкой: проверка только читает систему.
        ActionKind::Run => {
            let Some(check) = &a.check else { return Status::Unknown };
            match os::run(a.shell.as_deref().unwrap_or("cmd"), check) {
                Ok(out) if check_says_applied(&out) => Status::Applied,
                Ok(_) => Status::Pending,
                Err(_) => Status::Unknown,
            }
        }
        _ => Status::Unknown,
    }
}

/// Применяет действие к живой системе. Возвращает записи отката.
pub fn apply(a: &Action) -> Result<Vec<Undo>, String> {
    match a.kind() {
        ActionKind::Reg => apply_reg(a),
        ActionKind::Service => apply_service(a),
        ActionKind::Task => apply_task(a),
        ActionKind::Feature => apply_feature(a),
        ActionKind::Appx => remove_appx(&[a.appx.clone().ok_or("немає назви пакета")?]),
        ActionKind::Capability => apply_capability(a),
        ActionKind::Run => apply_run(a),
        ActionKind::Unknown => Err("невідома дія".into()),
    }
}

fn apply_reg(a: &Action) -> Result<Vec<Undo>, String> {
    let (root, sub) = a
        .reg
        .as_deref()
        .and_then(reg::split)
        .ok_or_else(|| format!("поганий шлях реєстру: {:?}", a.reg))?;
    let name = a.name.clone().unwrap_or_default();

    // Уже так, как нужно, — ничего не пишем и в журнал не добавляем. Так уже
    // применённый твик не падает на защищённом ключе (например, политика виджетов
    // Dsh, которую установщик записал от SYSTEM), а журнал отката хранит только
    // настоящие изменения.
    if status(a) == Status::Applied {
        return Ok(Vec::new());
    }

    if a.delete && a.name.is_none() {
        // Удаление целого ключа: откат не поддержан.
        reg::delete_key(root, &sub).map_err(|e| e.to_string())?;
        return Ok(Vec::new());
    }

    let mut undos = Vec::new();
    let prev = reg::read_raw(root, &sub, &name);
    if a.delete {
        reg::delete_value(root, &sub, &name).map_err(|e| e.to_string())?;
    } else {
        let data = value_of(a).ok_or("у реєстрового запису немає значення")?;
        // Запоминаем самый верхний ключ, которого ещё нет: при откате удалим его.
        if let Some(created) = topmost_missing(root, &sub) {
            undos.push(Undo::KeyCreated {
                root,
                subpath: created,
            });
        }
        reg::write(root, &sub, &name, &data).map_err(|e| e.to_string())?;
    }
    undos.push(Undo::Reg {
        root,
        subpath: sub,
        name,
        prev,
    });
    Ok(undos)
}

/// Самый верхний отсутствующий ключ на пути (тот, что создаст запись).
fn topmost_missing(root: Root, sub: &str) -> Option<String> {
    let parts: Vec<&str> = sub.split('\\').filter(|p| !p.is_empty()).collect();
    for i in 1..=parts.len() {
        let prefix = parts[..i].join("\\");
        if !reg::key_exists(root, &prefix) {
            return Some(prefix);
        }
    }
    None
}

fn apply_service(a: &Action) -> Result<Vec<Undo>, String> {
    let svc = a.service.clone().ok_or("немає назви служби")?;
    let (start, delayed) = service_start(a.start.as_deref().unwrap_or("disabled"));
    let sub = format!(r"SYSTEM\CurrentControlSet\Services\{svc}");
    // Такой службы нет — не создаём ключ, просто пропускаем.
    if !reg::key_exists(Root::Hklm, &sub) {
        return Ok(Vec::new());
    }
    let mut undos = Vec::new();
    for (name, val) in [("Start", start), ("DelayedAutostart", delayed)] {
        let prev = reg::read_raw(Root::Hklm, &sub, name);
        // Уже нужное значение (отсутствующий DelayedAutostart — это 0) — не трогаем.
        let current = match &prev {
            Some(raw) if raw.bytes.len() >= 4 => Some(u32::from_le_bytes([raw.bytes[0], raw.bytes[1], raw.bytes[2], raw.bytes[3]])),
            None if name == "DelayedAutostart" => Some(0),
            _ => None,
        };
        if current == Some(val) {
            continue;
        }
        reg::write(Root::Hklm, &sub, name, &ValueData::Dword(val)).map_err(|e| e.to_string())?;
        undos.push(Undo::Reg {
            root: Root::Hklm,
            subpath: sub.clone(),
            name: name.into(),
            prev,
        });
    }
    // Попытаться остановить службу сейчас (без отката; можно запустить обратно).
    if start >= 3 {
        let _ = os::run("cmd", &format!("sc stop \"{svc}\""));
    }
    Ok(undos)
}

/// Включена ли задача планировщика (None — такой задачи нет).
/// XML задачи не зависит от языка системы: `<Settings>…<Enabled>false</Enabled>`.
fn task_enabled(task: &str) -> Option<bool> {
    let xml = os::run("cmd", &format!("schtasks /Query /TN \"{task}\" /XML")).ok()?;
    Some(task_xml_enabled(&xml))
}

fn task_xml_enabled(xml: &str) -> bool {
    let settings = match (xml.find("<Settings>"), xml.find("</Settings>")) {
        (Some(s), Some(e)) if s < e => &xml[s..e],
        _ => xml,
    };
    !settings.contains("<Enabled>false</Enabled>")
}

fn apply_task(a: &Action) -> Result<Vec<Undo>, String> {
    let task = a.task.clone().ok_or("немає назви завдання")?;
    let enable = a.enabled.unwrap_or(false);
    // Задачи нет или она уже в нужном состоянии — ничего не меняем и в журнал не пишем:
    // иначе откат «вернул» бы то, чего до WinForge не было (например, включил телеметрию).
    match task_enabled(&task) {
        None => return Ok(Vec::new()),
        Some(state) if state == enable => return Ok(Vec::new()),
        Some(_) => {}
    }
    let flag = if enable { "/Enable" } else { "/Disable" };
    match os::run("cmd", &format!("schtasks /Change /TN \"{task}\" {flag}")) {
        Ok(_) => Ok(vec![Undo::Task { task, enable: !enable }]),
        Err(_) => Ok(Vec::new()),
    }
}

/// Состояние компонента Windows по `dism /English /Get-FeatureInfo`:
/// Some(true) — включён (или включится после перезагрузки), Some(false) — выключен,
/// None — такого компонента нет или узнать не удалось.
fn feature_enabled(feature: &str) -> Option<bool> {
    let out = os::run(
        "cmd",
        &format!("dism /English /Online /Get-FeatureInfo /FeatureName:{feature}"),
    )
    .ok()?;
    feature_info_enabled(&out)
}

fn feature_info_enabled(out: &str) -> Option<bool> {
    let state = out
        .lines()
        .find_map(|l| l.trim().strip_prefix("State :").or_else(|| l.trim().strip_prefix("State:")))?
        .trim()
        .to_lowercase();
    Some(state.starts_with("enable"))
}

fn apply_feature(a: &Action) -> Result<Vec<Undo>, String> {
    let feature = a.feature.clone().ok_or("немає назви компонента")?;
    let enable = a.enabled.unwrap_or(false);
    // Уже в нужном состоянии — ничего не делаем и в журнал не пишем (иначе откат
    // включил бы, например, SMB1, который был выключен ещё до WinForge).
    if feature_enabled(&feature) == Some(enable) {
        return Ok(Vec::new());
    }
    let (verb, all) = if enable { ("/Enable-Feature", " /All") } else { ("/Disable-Feature", "") };
    match os::run(
        "cmd",
        &format!("dism /English /Online {verb} /FeatureName:{feature}{all} /NoRestart /Quiet"),
    ) {
        Ok(_) => {}
        // 3010 — готово, нужна перезагрузка.
        Err(e) if e.starts_with("код 3010") => {}
        // Такого компонента в этой версии Windows нет (например, PowerShell 2.0 в 24H2).
        Err(e) if !enable && e.to_lowercase().contains("0x800f080c") => return Ok(Vec::new()),
        Err(e) => return Err(e),
    }
    Ok(vec![Undo::Feature {
        feature,
        enable: !enable,
    }])
}

/// Удаляет пакеты Appx у всех пользователей и из образа подготовки — одним
/// запуском PowerShell (по пакету на запуск выходило бы по секунде на каждый).
/// Имена можно с масками: `*CandyCrush*`. Откат невозможен.
pub fn remove_appx(packages: &[String]) -> Result<Vec<Undo>, String> {
    if packages.is_empty() {
        return Ok(Vec::new());
    }
    let list = packages.iter().map(|p| ps_quote(p)).collect::<Vec<_>>().join(", ");
    let script = format!(
        "$prov = @(Get-AppxProvisionedPackage -Online); \
         foreach ($name in @({list})) {{ \
         Get-AppxPackage -AllUsers -Name $name | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue; \
         $prov | Where-Object {{ $_.DisplayName -like $name }} | \
         ForEach-Object {{ Remove-AppxProvisionedPackage -Online -PackageName $_.PackageName -ErrorAction SilentlyContinue | Out-Null }} }}"
    );
    os::run("powershell", &script)?;
    Ok(Vec::new())
}

/// Строка в одинарных кавычках PowerShell.
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Удаляет возможность Windows (все её версии и языки). Откат — вернуть именно
/// удалённые (Add-WindowsCapability скачает их из Windows Update).
fn apply_capability(a: &Action) -> Result<Vec<Undo>, String> {
    let cap = a.capability.clone().ok_or("немає назви можливості")?;
    let pattern = ps_quote(&format!("{cap}~*"));
    let out = os::run(
        "powershell",
        &format!(
            "$ErrorActionPreference = 'Stop'; \
             foreach ($c in @(Get-WindowsCapability -Online | Where-Object {{ $_.Name -like {pattern} -and $_.State -eq 'Installed' }})) {{ \
             Remove-WindowsCapability -Online -Name $c.Name | Out-Null; $c.Name }}"
        ),
    )?;
    let removed: Vec<String> = out.lines().map(str::trim).filter(|l| !l.is_empty()).map(ps_quote).collect();
    if removed.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![Undo::Run {
        cmd: format!(
            "foreach ($n in @({})) {{ Add-WindowsCapability -Online -Name $n | Out-Null }}",
            removed.join(", ")
        ),
        shell: "powershell".into(),
    }])
}

fn apply_run(a: &Action) -> Result<Vec<Undo>, String> {
    let cmd = a.run.clone().ok_or("немає команди")?;
    let shell = a.shell.clone().unwrap_or_else(|| "cmd".into());
    // Уже применено — ничего не делаем, иначе откат «вернул» бы то, чего не было
    // (например, сбросил бы план питания, выбранный ещё до WinForge).
    if let Some(check) = &a.check {
        if check_says_applied(&os::run(&shell, check).unwrap_or_default()) {
            return Ok(Vec::new());
        }
    }
    os::run(&shell, &cmd)?;
    match &a.undo {
        Some(undo) => Ok(vec![Undo::Run {
            cmd: undo.clone(),
            shell,
        }]),
        None => Ok(Vec::new()),
    }
}

/// Вывод проверки `check`: последняя непустая строка — `True`.
fn check_says_applied(out: &str) -> bool {
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .last()
        .map(|l| l.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Откатывает одну запись.
pub fn revert(u: &Undo) -> Result<(), String> {
    match u {
        Undo::Reg {
            root,
            subpath,
            name,
            prev,
        } => reg::restore(*root, subpath, name, prev).map_err(|e| e.to_string()),
        Undo::KeyCreated { root, subpath } => reg::delete_key(*root, subpath).map_err(|e| e.to_string()),
        Undo::Task { task, enable } => {
            let flag = if *enable { "/Enable" } else { "/Disable" };
            os::run("cmd", &format!("schtasks /Change /TN \"{task}\" {flag}")).map(|_| ())
        }
        Undo::Feature { feature, enable } => {
            let verb = if *enable { "/Enable-Feature" } else { "/Disable-Feature" };
            os::run(
                "cmd",
                &format!("dism /Online {verb} /FeatureName:{feature} /NoRestart /Quiet"),
            )
            .map(|_| ())
        }
        Undo::Run { cmd, shell } => os::run(shell, cmd).map(|_| ()),
    }
}

// ---------------------------------------------------------------------------
// Запуск внешних команд без окна консоли
// ---------------------------------------------------------------------------

pub mod os {
    #[cfg(windows)]
    const NO_WINDOW: u32 = 0x0800_0000; // CREATE_NO_WINDOW

    /// Запускает команду в cmd или powershell, возвращает stdout либо текст ошибки.
    #[cfg(windows)]
    pub fn run(shell: &str, command: &str) -> Result<String, String> {
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        let mut c = if shell.eq_ignore_ascii_case("powershell") {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", command]);
            c
        } else {
            let mut c = Command::new("cmd");
            c.args(["/C", command]);
            c
        };
        c.creation_flags(NO_WINDOW);
        // Не из папки WinForge: если exe лежит на сетевом пути (\\server\share),
        // cmd.exe ругается на UNC-путь в качестве текущей папки.
        if let Some(root) = std::env::var_os("SystemRoot") {
            c.current_dir(std::path::PathBuf::from(root).join("System32"));
        }
        let out = c.output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            let msg = if err.trim().is_empty() {
                String::from_utf8_lossy(&out.stdout).into_owned()
            } else {
                err.into_owned()
            };
            Err(format!("код {}: {}", out.status.code().unwrap_or(-1), msg.trim()))
        }
    }

    #[cfg(not(windows))]
    pub fn run(_shell: &str, _command: &str) -> Result<String, String> {
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_start_mapping() {
        assert_eq!(service_start("disabled"), (4, 0));
        assert_eq!(service_start("delayed"), (2, 1));
        assert_eq!(service_start("auto"), (2, 0));
        assert_eq!(service_start("manual"), (3, 0));
    }

    #[test]
    fn check_output_is_read_from_last_line() {
        assert!(check_says_applied("True\r\n"));
        assert!(check_says_applied("warning\r\nTrue\r\n\r\n"));
        assert!(!check_says_applied("False"));
        assert!(!check_says_applied(""));
    }

    #[test]
    fn capability_undo_is_described_by_name() {
        let u = Undo::Run {
            cmd: "foreach ($n in @('Media.WindowsMediaPlayer~~~~0.0.12.0')) { Add-WindowsCapability -Online -Name $n | Out-Null }".into(),
            shell: "powershell".into(),
        };
        assert!(u.describe(crate::i18n::Lang::Uk).starts_with("Компонент Media.WindowsMediaPlayer — завантаження з Windows Update"));
        assert!(u.describe(crate::i18n::Lang::En).starts_with("Feature Media.WindowsMediaPlayer — downloading"));
    }

    #[test]
    fn task_state_comes_from_settings_only() {
        let disabled = "<Task><Triggers><BootTrigger><Enabled>true</Enabled></BootTrigger></Triggers>\
                        <Settings><Enabled>false</Enabled><Hidden>true</Hidden></Settings></Task>";
        assert!(!task_xml_enabled(disabled));
        // Выключенный триггер не значит выключенную задачу.
        let enabled = "<Task><Triggers><TimeTrigger><Enabled>false</Enabled></TimeTrigger></Triggers>\
                       <Settings><Hidden>true</Hidden></Settings></Task>";
        assert!(task_xml_enabled(enabled));
    }

    #[test]
    fn feature_state_is_parsed() {
        let info = "Deployment Image Servicing and Management tool\r\n\r\nFeature Name : SMB1Protocol\r\nDisplay Name : SMB 1.0\r\nState : Disabled\r\n";
        assert_eq!(feature_info_enabled(info), Some(false));
        assert_eq!(feature_info_enabled("State : Enable Pending"), Some(true));
        assert_eq!(feature_info_enabled("State : Disabled with Payload Removed"), Some(false));
        assert_eq!(feature_info_enabled("Error: 0x800f080c"), None);
    }

    #[test]
    fn value_of_reads_each_kind() {
        let mut a = Action::default();
        a.dword = Some(1);
        assert!(matches!(value_of(&a), Some(ValueData::Dword(1))));
        let mut a = Action::default();
        a.binary = Some("aa bb".into());
        assert!(matches!(value_of(&a), Some(ValueData::Binary(_))));
    }
}
