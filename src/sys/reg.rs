//! Чтение и запись реестра Windows с сохранением предыдущего значения
//! (для отката). Один модуль используется и для живой системы, и для
//! смонтированного офлайн-образа — разница только в базовом ключе.

use serde::{Deserialize, Serialize};

#[cfg(windows)]
use std::io;

/// Корневой куст реестра.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Root {
    Hklm,
    Hkcu,
    Hkcr,
    Hku,
}


/// Разбирает путь вида `HKLM\SOFTWARE\...` на корень и остаток.
pub fn split(path: &str) -> Option<(Root, String)> {
    let path = path.trim().replace('/', "\\");
    let (head, rest) = match path.split_once('\\') {
        Some((h, r)) => (h.to_string(), r.to_string()),
        None => (path.clone(), String::new()),
    };
    let root = match head.to_uppercase().as_str() {
        "HKLM" | "HKEY_LOCAL_MACHINE" => Root::Hklm,
        "HKCU" | "HKEY_CURRENT_USER" => Root::Hkcu,
        "HKCR" | "HKEY_CLASSES_ROOT" => Root::Hkcr,
        "HKU" | "HKEY_USERS" => Root::Hku,
        _ => return None,
    };
    Some((root, rest))
}

/// Значение реестра для записи.
#[derive(Debug, Clone)]
pub enum ValueData {
    Dword(u32),
    Qword(u64),
    Sz(String),
    Expand(String),
    Multi(Vec<String>),
    Binary(Vec<u8>),
}

/// Сериализуемое сырое значение (для журнала отката).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawValue {
    /// Код типа REG_* (winnt.h).
    pub vtype: u32,
    pub bytes: Vec<u8>,
}

/// Разбирает строку шестнадцатеричных байт («02 00 1a») в вектор.
pub fn parse_hex(s: &str) -> Vec<u8> {
    s.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|p| !p.is_empty())
        .filter_map(|p| u8::from_str_radix(p.trim_start_matches("0x"), 16).ok())
        .collect()
}

// Коды типов REG_*.
pub const REG_SZ: u32 = 1;
pub const REG_EXPAND_SZ: u32 = 2;
pub const REG_BINARY: u32 = 3;
pub const REG_DWORD: u32 = 4;
pub const REG_MULTI_SZ: u32 = 7;
pub const REG_QWORD: u32 = 11;

fn utf16_nul(s: &str) -> Vec<u8> {
    let mut v: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    v.push(0);
    v.push(0);
    v
}

impl ValueData {
    /// (тип, байты) для записи в реестр.
    pub fn raw(&self) -> RawValue {
        let (vtype, bytes) = match self {
            ValueData::Dword(v) => (REG_DWORD, v.to_le_bytes().to_vec()),
            ValueData::Qword(v) => (REG_QWORD, v.to_le_bytes().to_vec()),
            ValueData::Sz(s) => (REG_SZ, utf16_nul(s)),
            ValueData::Expand(s) => (REG_EXPAND_SZ, utf16_nul(s)),
            ValueData::Multi(list) => {
                let mut bytes = Vec::new();
                for s in list {
                    bytes.extend(s.encode_utf16().flat_map(|u| u.to_le_bytes()));
                    bytes.extend_from_slice(&[0, 0]);
                }
                bytes.extend_from_slice(&[0, 0]);
                (REG_MULTI_SZ, bytes)
            }
            ValueData::Binary(b) => (REG_BINARY, b.clone()),
        };
        RawValue { vtype, bytes }
    }
}

// ---------------------------------------------------------------------------
// Реализация под Windows
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub use win::*;

#[cfg(windows)]
mod win {
    use super::*;
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    fn hkey(root: Root) -> RegKey {
        let h = match root {
            Root::Hklm => HKEY_LOCAL_MACHINE,
            Root::Hkcu => HKEY_CURRENT_USER,
            Root::Hkcr => HKEY_CLASSES_ROOT,
            Root::Hku => HKEY_USERS,
        };
        RegKey::predef(h)
    }

    /// Читает текущее сырое значение (None, если его нет).
    pub fn read_raw(root: Root, subpath: &str, name: &str) -> Option<RawValue> {
        let key = hkey(root).open_subkey_with_flags(subpath, KEY_READ).ok()?;
        let v = key.get_raw_value(name).ok()?;
        Some(RawValue {
            vtype: v.vtype as u32,
            bytes: v.bytes,
        })
    }

    /// Существует ли ключ.
    pub fn key_exists(root: Root, subpath: &str) -> bool {
        hkey(root).open_subkey_with_flags(subpath, KEY_READ).is_ok()
    }

    fn reg_type(code: u32) -> RegType {
        match code {
            super::REG_SZ => RegType::REG_SZ,
            super::REG_EXPAND_SZ => RegType::REG_EXPAND_SZ,
            super::REG_BINARY => RegType::REG_BINARY,
            super::REG_DWORD => RegType::REG_DWORD,
            super::REG_MULTI_SZ => RegType::REG_MULTI_SZ,
            super::REG_QWORD => RegType::REG_QWORD,
            _ => RegType::REG_NONE,
        }
    }

    /// Записывает значение, создавая ключ при необходимости.
    pub fn write(root: Root, subpath: &str, name: &str, data: &ValueData) -> io::Result<()> {
        let (key, _) = hkey(root).create_subkey(subpath)?;
        let raw = data.raw();
        key.set_raw_value(
            name,
            &RegValue {
                vtype: reg_type(raw.vtype),
                bytes: raw.bytes,
            },
        )
    }

    /// Восстанавливает значение из журнала (или удаляет, если его не было).
    pub fn restore(root: Root, subpath: &str, name: &str, prev: &Option<RawValue>) -> io::Result<()> {
        match prev {
            Some(raw) => {
                let (key, _) = hkey(root).create_subkey(subpath)?;
                key.set_raw_value(
                    name,
                    &RegValue {
                        vtype: reg_type(raw.vtype),
                        bytes: raw.bytes.clone(),
                    },
                )
            }
            None => delete_value(root, subpath, name),
        }
    }

    pub fn delete_value(root: Root, subpath: &str, name: &str) -> io::Result<()> {
        match hkey(root).open_subkey_with_flags(subpath, KEY_SET_VALUE) {
            Ok(key) => match key.delete_value(name) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub fn delete_key(root: Root, subpath: &str) -> io::Result<()> {
        match hkey(root).delete_subkey_all(subpath) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Заглушки для сборки не под Windows (разработка/проверка синтаксиса)
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
pub use stub::*;

#[cfg(not(windows))]
mod stub {
    use super::*;

    pub fn read_raw(_: Root, _: &str, _: &str) -> Option<RawValue> {
        None
    }
    pub fn key_exists(_: Root, _: &str) -> bool {
        false
    }
    pub fn write(_: Root, _: &str, _: &str, _: &ValueData) -> std::io::Result<()> {
        Ok(())
    }
    pub fn restore(_: Root, _: &str, _: &str, _: &Option<RawValue>) -> std::io::Result<()> {
        Ok(())
    }
    pub fn delete_value(_: Root, _: &str, _: &str) -> std::io::Result<()> {
        Ok(())
    }
    pub fn delete_key(_: Root, _: &str) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_roots() {
        assert_eq!(split(r"HKLM\SOFTWARE\X"), Some((Root::Hklm, r"SOFTWARE\X".into())));
        assert_eq!(
            split(r"HKEY_CURRENT_USER\Software\Y"),
            Some((Root::Hkcu, r"Software\Y".into()))
        );
        assert!(split(r"BOGUS\x").is_none());
    }

    #[test]
    fn dword_bytes() {
        assert_eq!(ValueData::Dword(1).raw().bytes, vec![1, 0, 0, 0]);
        assert_eq!(ValueData::Dword(0).raw().vtype, REG_DWORD);
    }

    #[test]
    fn sz_is_utf16_nul_terminated() {
        let raw = ValueData::Sz("Hi".into()).raw();
        assert_eq!(raw.vtype, REG_SZ);
        assert_eq!(raw.bytes, vec![b'H', 0, b'i', 0, 0, 0]);
    }

    #[test]
    fn multi_sz_double_nul() {
        let raw = ValueData::Multi(vec!["a".into(), "b".into()]).raw();
        assert_eq!(raw.vtype, REG_MULTI_SZ);
        assert_eq!(raw.bytes, vec![b'a', 0, 0, 0, b'b', 0, 0, 0, 0, 0]);
    }

    #[test]
    fn hex_parse() {
        assert_eq!(parse_hex("02 00 1a ff"), vec![0x02, 0x00, 0x1a, 0xff]);
        assert_eq!(parse_hex("0x0A,0x0B"), vec![0x0a, 0x0b]);
    }
}
