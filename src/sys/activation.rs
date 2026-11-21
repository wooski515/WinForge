//! Активация Windows своим ключом: установка ключа продукта, адрес KMS-сервера
//! своей организации и текущее состояние лицензии.
//!
//! Работает через WMI-службу лицензирования (`SoftwareLicensingService` /
//! `SoftwareLicensingProduct`) — то же, что делает slmgr.vbs, но без вывода,
//! зависящего от языка системы. Публичных KMS-серверов здесь нет: адрес вводит
//! пользователь (сервер его компании или учебного заведения).

use crate::i18n::Lang;
use crate::sys::engine::os;

/// ApplicationID самой Windows в службе лицензирования.
const WINDOWS_APP_ID: &str = "55c92734-d682-4d71-983e-d6ec3f16059f";

/// Состояние лицензии Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// Название канала/редакции, например "Windows(R), Professional edition".
    pub name: String,
    /// LicenseStatus: 0 — не лицензирована, 1 — активирована, 2–6 — разные пробные периоды.
    pub license: u32,
    /// Последние 5 символов установленного ключа.
    pub partial_key: String,
    /// Сколько минут осталось до конца пробного периода / KMS-активации.
    pub grace_minutes: u64,
    /// Адрес KMS-сервера, заданный вручную (пусто — ищется автоматически).
    pub kms: String,
}

impl Status {
    pub fn activated(&self) -> bool {
        self.license == 1
    }

    pub fn text(&self, l: Lang) -> &'static str {
        match self.license {
            1 => l.tr("Активовано", "Activated"),
            0 => l.tr("Не активовано", "Not activated"),
            2 | 3 | 6 => l.tr("Пробний період", "Grace period"),
            4 => l.tr("Ліцензію не підтверджено", "License not genuine"),
            5 => l.tr("Не активовано (сповіщення)", "Not activated (notifications)"),
            _ => l.tr("Невідомо", "Unknown"),
        }
    }
}

/// Ключ вида XXXXX-XXXXX-XXXXX-XXXXX-XXXXX (буквы и цифры). Возвращает ключ в верхнем регистре.
pub fn normalize_key(key: &str) -> Option<String> {
    let key = key.trim().to_uppercase();
    let groups: Vec<&str> = key.split('-').collect();
    let ok = groups.len() == 5
        && groups
            .iter()
            .all(|g| g.len() == 5 && g.chars().all(|c| c.is_ascii_alphanumeric()));
    ok.then_some(key)
}

/// Адрес KMS-сервера: имя или IP, по желанию с портом (`kms.company.local:1688`).
/// Возвращает (адрес, порт).
pub fn parse_kms_host(host: &str) -> Option<(String, Option<u16>)> {
    let host = host.trim();
    let (name, port) = match host.rsplit_once(':') {
        Some((n, p)) => (n, Some(p.parse::<u16>().ok().filter(|p| *p > 0)?)),
        None => (host, None),
    };
    let ok = !name.is_empty()
        && name.len() <= 253
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !name.starts_with('-')
        && !name.starts_with('.');
    ok.then(|| (name.to_owned(), port))
}

/// Начало каждого скрипта: UTF-8 на выходе и объекты службы и продукта Windows.
fn prelude() -> String {
    format!(
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; $ErrorActionPreference = 'Stop'; \
         $svc = Get-CimInstance SoftwareLicensingService; \
         function Get-WinProduct {{ Get-CimInstance SoftwareLicensingProduct -Filter \"ApplicationID='{WINDOWS_APP_ID}' AND PartialProductKey IS NOT NULL\" | Select-Object -First 1 }}; "
    )
}

const STATUS_LINE: &str = "$p = Get-WinProduct; if ($p) { 'STATUS|' + $p.Name + '|' + $p.LicenseStatus + '|' + $p.PartialProductKey + '|' + $p.GracePeriodRemaining + '|' + $svc.KeyManagementServiceMachine } else { 'NONE' }";

/// Скрипт: состояние лицензии.
pub(crate) fn status_script() -> String {
    format!("{}{STATUS_LINE}", prelude())
}

/// Скрипт: (по желанию) поставить ключ, (по желанию) задать KMS-сервер, активировать.
pub(crate) fn activate_script(key: Option<&str>, kms: Option<(&str, Option<u16>)>) -> String {
    let mut s = prelude();
    s.push_str("try { ");
    if let Some(key) = key {
        s.push_str(&format!(
            "Invoke-CimMethod -InputObject $svc -MethodName InstallProductKey -Arguments @{{ ProductKey = '{key}' }} | Out-Null; "
        ));
    }
    if let Some((host, port)) = kms {
        s.push_str(&format!(
            "Invoke-CimMethod -InputObject $svc -MethodName SetKeyManagementServiceMachine -Arguments @{{ MachineName = '{host}' }} | Out-Null; "
        ));
        if let Some(port) = port {
            s.push_str(&format!(
                "Invoke-CimMethod -InputObject $svc -MethodName SetKeyManagementServicePort -Arguments @{{ PortNumber = [uint32]{port} }} | Out-Null; "
            ));
        }
    }
    s.push_str(
        "Invoke-CimMethod -InputObject $svc -MethodName RefreshLicenseStatus | Out-Null; \
         $p = Get-WinProduct; if (-not $p) { throw 'No Windows product key is installed' }; \
         Invoke-CimMethod -InputObject $p -MethodName Activate | Out-Null; \
         Invoke-CimMethod -InputObject $svc -MethodName RefreshLicenseStatus | Out-Null; 'OK' \
         } catch { 'ERR|' + ('0x{0:X8}' -f $_.Exception.HResult) + '|' + $_.Exception.Message }; ",
    );
    s.push_str(STATUS_LINE);
    s
}

/// Разбор вывода скриптов.
fn parse(out: &str) -> (Option<String>, Option<Status>) {
    let mut error = None;
    let mut status = None;
    for line in out.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("ERR|") {
            error = Some(rest.to_owned());
        } else if let Some(rest) = line.strip_prefix("STATUS|") {
            let f: Vec<&str> = rest.split('|').collect();
            if f.len() >= 5 {
                status = Some(Status {
                    name: f[0].to_owned(),
                    license: f[1].parse().unwrap_or(99),
                    partial_key: f[2].to_owned(),
                    grace_minutes: f[3].parse().unwrap_or(0),
                    kms: f.get(4).copied().unwrap_or("").to_owned(),
                });
            }
        }
    }
    (error, status)
}

/// Понятное объяснение кода ошибки службы лицензирования.
pub fn explain(error: &str, l: Lang) -> String {
    let code = find_code(error);
    let known = match code.as_deref() {
        Some("0xC004F050") => Some(l.tr("Ключ не підходить: неправильний або для іншої редакції Windows.", "The key doesn't fit: wrong, or for another Windows edition.")),
        Some("0xC004C003") => Some(l.tr("Цей ключ заблоковано Microsoft.", "Microsoft has blocked this key.")),
        Some("0xC004C008") | Some("0xC004C020") => Some(l.tr("Ключ уже використано на максимальній кількості ПК.", "The key has been used on the maximum number of PCs.")),
        Some("0xC004F074") => Some(l.tr("KMS-сервер недоступний. Перевір адресу й підключення до мережі організації.", "The KMS server can't be reached. Check the address and the connection to your organization's network.")),
        Some("0x8007232B") => Some(l.tr("KMS-сервер не знайдено автоматично. Вкажи його адресу вручну.", "No KMS server was found automatically. Enter its address.")),
        Some("0xC004F038") => Some(l.tr("На KMS-сервері замало комп'ютерів для активації.", "The KMS server doesn't have enough computers to activate yet.")),
        Some("0x80072EE7") | Some("0x80072EFD") | Some("0x80072F8F") => Some(l.tr("Немає зв'язку із серверами активації. Перевір інтернет і час на ПК.", "Can't reach the activation servers. Check the internet and the PC clock.")),
        Some("0x80041010") => Some(l.tr("Служба ліцензування не підтримує цю дію.", "The licensing service doesn't support this action.")),
        _ => None,
    };
    match (known, code) {
        (Some(text), Some(code)) => format!("{text} ({code})"),
        _ => error.to_owned(),
    }
}

/// Первый код вида 0xC004F050 / 0x8007232B в тексте.
fn find_code(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("0x") {
        let start = from + i;
        let hex: String = lower[start + 2..].chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        if hex.len() == 8 && hex != "80131500" && hex != "80041001" {
            return Some(format!("0x{}", hex.to_uppercase()));
        }
        from = start + 2;
    }
    None
}

/// Текущее состояние лицензии (None — ключ не установлен).
pub fn status() -> Result<Option<Status>, String> {
    let out = os::run("powershell", &status_script())?;
    Ok(parse(&out).1)
}

/// Ставит ключ и/или KMS-сервер и активирует. Ok — состояние после попытки.
pub fn activate(key: Option<&str>, kms: Option<(&str, Option<u16>)>) -> Result<Option<Status>, String> {
    let out = os::run("powershell", &activate_script(key, kms))?;
    match parse(&out) {
        (Some(error), _) => Err(error),
        (None, status) => Ok(status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_validated() {
        assert_eq!(
            normalize_key(" abcde-12345-fghij-67890-klmno ").as_deref(),
            Some("ABCDE-12345-FGHIJ-67890-KLMNO")
        );
        assert!(normalize_key("ABCDE-12345-FGHIJ-67890").is_none());
        assert!(normalize_key("ABCDE-12345-FGHIJ-67890-KLMN'").is_none());
        assert!(normalize_key("ABCDE-12345-FGHIJ-67890-KLMNO; calc").is_none());
    }

    #[test]
    fn kms_hosts_are_validated() {
        assert_eq!(parse_kms_host("kms.company.local"), Some(("kms.company.local".into(), None)));
        assert_eq!(parse_kms_host("10.0.0.5:1688"), Some(("10.0.0.5".into(), Some(1688))));
        assert!(parse_kms_host("").is_none());
        assert!(parse_kms_host("kms.local:0").is_none());
        assert!(parse_kms_host("kms'; calc").is_none());
        assert!(parse_kms_host("-x").is_none());
    }

    #[test]
    fn output_is_parsed() {
        let (err, st) = parse("OK\r\nSTATUS|Windows(R), Professional edition|1|3V66T|0|kms.company.local\r\n");
        assert!(err.is_none());
        let st = st.unwrap();
        assert!(st.activated());
        assert_eq!(st.partial_key, "3V66T");
        assert_eq!(st.kms, "kms.company.local");
        let (err, _) = parse("ERR|0xC004F050|Помилка 0xC004F050\r\nNONE\r\n");
        assert!(err.unwrap().contains("0xC004F050"));
    }

    #[test]
    fn errors_are_explained() {
        let t = explain("0x80131500|Exception 0xC004F050 ...", Lang::En);
        assert!(t.starts_with("The key doesn't fit") && t.ends_with("(0xC004F050)"), "{t}");
        assert_eq!(explain("something odd", Lang::En), "something odd");
    }

    #[test]
    fn scripts_only_contain_validated_values() {
        let s = activate_script(Some("ABCDE-12345-FGHIJ-67890-KLMNO"), Some(("kms.company.local", Some(1688))));
        assert!(s.contains("ProductKey = 'ABCDE-12345-FGHIJ-67890-KLMNO'"));
        assert!(s.contains("MachineName = 'kms.company.local'"));
        assert!(s.contains("PortNumber = [uint32]1688"));
        assert!(s.find("InstallProductKey").unwrap() < s.find("MethodName Activate").unwrap());
        let plain = activate_script(None, None);
        assert!(!plain.contains("InstallProductKey") && !plain.contains("SetKeyManagementServiceMachine"));
    }
}
