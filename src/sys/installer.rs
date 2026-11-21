//! Свои установщики: поиск в папке и определение типа, чтобы подобрать
//! ключи тихой установки (без окон и вопросов).
//!
//! Тип узнаётся по сигнатурам в начале файла: так делают и другие инструменты,
//! ведь у .exe-установщика нет единого стандарта ключей.

use std::path::{Path, PathBuf};

/// Сколько байт читать с начала файла: заглушки установщиков и их метки
/// лежат в первых мегабайтах, а сами файлы бывают по гигабайту.
const SCAN_BYTES: usize = 8 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallerKind {
    Msi,
    Inno,
    Nsis,
    InstallShield,
    WixBurn,
    AdvancedInstaller,
    Squirrel,
    Unknown,
}

impl InstallerKind {
    /// Ключи тихой установки. Пустая строка — неизвестно, нужно указать самому.
    pub fn silent_args(self) -> &'static str {
        match self {
            InstallerKind::Msi => "/qn /norestart",
            InstallerKind::Inno => "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-",
            InstallerKind::Nsis => "/S",
            InstallerKind::InstallShield => "/s /v\"/qn /norestart\"",
            InstallerKind::WixBurn => "/quiet /norestart",
            InstallerKind::AdvancedInstaller => "/exenoui /qn /norestart",
            InstallerKind::Squirrel => "--silent",
            InstallerKind::Unknown => "",
        }
    }
}

fn contains(data: &[u8], needle: &[u8]) -> bool {
    data.windows(needle.len()).any(|w| w == needle)
}

/// Тип по имени и первым байтам файла.
pub fn detect_bytes(file_name: &str, data: &[u8]) -> InstallerKind {
    if file_name.to_lowercase().ends_with(".msi") {
        return InstallerKind::Msi;
    }
    // Порядок важен: Burn и Advanced Installer внутри часто несут MSI и чужие строки.
    if contains(data, b".wixburn") {
        InstallerKind::WixBurn
    } else if contains(data, b"Inno Setup") || contains(data, b"InnoSetupLdrWindow") {
        InstallerKind::Inno
    } else if contains(data, b"Nullsoft.NSIS") || contains(data, b"NullsoftInst") {
        InstallerKind::Nsis
    } else if contains(data, b"Advanced Installer") {
        InstallerKind::AdvancedInstaller
    } else if contains(data, b"InstallShield") {
        InstallerKind::InstallShield
    } else if contains(data, b"SquirrelSetup") || contains(data, b"Squirrel.Windows") {
        InstallerKind::Squirrel
    } else {
        InstallerKind::Unknown
    }
}

pub fn detect(path: &Path) -> InstallerKind {
    use std::io::Read;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut data = Vec::new();
    if let Ok(f) = std::fs::File::open(path) {
        let _ = f.take(SCAN_BYTES as u64).read_to_end(&mut data);
    }
    detect_bytes(&name, &data)
}

/// Установщик, найденный в папке.
#[derive(Clone, Debug)]
pub struct Found {
    pub path: PathBuf,
    /// Имя для списка: имя файла без расширения.
    pub name: String,
    pub kind: InstallerKind,
}

/// Все .exe и .msi в папке и её подпапках (до 2 уровней), по алфавиту.
pub fn scan_folder(dir: &Path) -> Vec<Found> {
    fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 2 {
                    walk(&p, depth + 1, out);
                }
            } else if p
                .extension()
                .map(|x| x.eq_ignore_ascii_case("exe") || x.eq_ignore_ascii_case("msi"))
                .unwrap_or(false)
            {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, 0, &mut files);
    files.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    files
        .into_iter()
        .map(|path| Found {
            name: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            kind: detect(&path),
            path,
        })
        .collect()
}

/// Имена файлов на носителе: исходные, а при совпадении — с номером спереди.
pub fn staged_names(paths: &[&str]) -> Vec<String> {
    let mut used: Vec<String> = Vec::new();
    paths
        .iter()
        .map(|p| {
            let base = Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "setup.exe".into());
            let mut name = base.clone();
            let mut n = 2;
            while used.iter().any(|u| u.eq_ignore_ascii_case(&name)) {
                name = format!("{n}_{base}");
                n += 1;
            }
            used.push(name.clone());
            name
        })
        .collect()
}

/// Ссылка, а не файл: такой установщик скачивается при первом входе.
pub fn is_url(value: &str) -> bool {
    let v = value.trim().to_lowercase();
    v.starts_with("http://") || v.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_signature() {
        let mut stub = vec![0u8; 4096];
        stub.extend_from_slice(b"...InnoSetupLdrWindow...");
        assert_eq!(detect_bytes("app.exe", &stub), InstallerKind::Inno);
        assert_eq!(detect_bytes("x.exe", b"MZ....NullsoftInst...."), InstallerKind::Nsis);
        assert_eq!(detect_bytes("x.exe", b"MZ .wixburn ... Inno Setup"), InstallerKind::WixBurn, "Burn раньше прочих");
        assert_eq!(detect_bytes("x.exe", b"MZ InstallShield"), InstallerKind::InstallShield);
        assert_eq!(detect_bytes("x.exe", b"MZ Advanced Installer InstallShield"), InstallerKind::AdvancedInstaller);
        assert_eq!(detect_bytes("x.exe", b"MZ SquirrelSetup"), InstallerKind::Squirrel);
        assert_eq!(detect_bytes("Tool.MSI", b""), InstallerKind::Msi);
        assert_eq!(detect_bytes("x.exe", b"MZ nothing here"), InstallerKind::Unknown);
        assert_eq!(InstallerKind::Unknown.silent_args(), "");
        assert_eq!(InstallerKind::Nsis.silent_args(), "/S");
    }

    /// Определение на настоящих файлах: WF_SCAN_DIR=<папка> cargo test scan_real -- --ignored --nocapture
    #[test]
    #[ignore = "читает папку из WF_SCAN_DIR"]
    fn scan_real_folder() {
        let Ok(dir) = std::env::var("WF_SCAN_DIR") else { return };
        for f in scan_folder(Path::new(&dir)) {
            println!("{:?}\t{}\t{}", f.kind, f.kind.silent_args(), f.path.display());
        }
    }

    #[test]
    fn same_file_names_get_numbers() {
        let names = staged_names(&[r"D:\a\setup.exe", r"D:\b\Setup.exe", r"D:\c\tool.msi", r"E:\setup.exe"]);
        assert_eq!(names, vec!["setup.exe", "2_Setup.exe", "tool.msi", "3_setup.exe"]);
    }

    #[test]
    fn scan_finds_exe_and_msi_in_subfolders() {
        let dir = std::env::temp_dir().join(format!("wf-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub").join("deep").join("deeper")).unwrap();
        std::fs::write(dir.join("b.exe"), b"MZ NullsoftInst").unwrap();
        std::fs::write(dir.join("a.msi"), b"").unwrap();
        std::fs::write(dir.join("readme.txt"), b"").unwrap();
        std::fs::write(dir.join("sub").join("c.EXE"), b"MZ").unwrap();
        std::fs::write(dir.join("sub").join("deep").join("deeper").join("d.exe"), b"MZ").unwrap();
        let found = scan_folder(&dir);
        let names: Vec<_> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"], "не глубже 2 уровней, только .exe/.msi");
        assert_eq!(found[1].kind, InstallerKind::Nsis);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
