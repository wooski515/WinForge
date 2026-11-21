//! Фоновое выполнение задач (применение твиков, очистка) в отдельном потоке
//! с живым прогрессом. Два режима: предпросмотр (только чтение состояния) и
//! реальное применение (с журналом отката).

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::catalog::Action;
use crate::sys::{self, clean, engine};

/// Итог одного шага (задаёт цвет и текст в интерфейсе).
#[derive(Clone)]
pub enum Outcome {
    /// Ещё не выполнен.
    Wait,
    /// Выполняется сейчас.
    Active,
    /// Успех / «уже применено» (зелёный).
    Good(String),
    /// Нейтрально: «будет изменено» / «пропущено» (жёлтый).
    Warn(String),
    /// Ошибка (красный).
    Bad(String),
}

#[derive(Clone)]
pub struct Step {
    pub label: String,
    pub outcome: Outcome,
}

#[derive(Clone)]
pub struct Progress {
    pub preview: bool,
    pub steps: Vec<Step>,
    pub finished: bool,
    pub summary: String,
}

impl Progress {
    fn new(preview: bool, labels: Vec<String>) -> Self {
        Self {
            preview,
            steps: labels
                .into_iter()
                .map(|label| Step {
                    label,
                    outcome: Outcome::Wait,
                })
                .collect(),
            finished: false,
            summary: String::new(),
        }
    }

    pub fn done_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| !matches!(s.outcome, Outcome::Wait | Outcome::Active))
            .count()
    }

    pub fn fraction(&self) -> f32 {
        let total = self.steps.len().max(1);
        self.done_count() as f32 / total as f32
    }
}

/// Единица работы для твиков: подпись + её действия.
pub struct Unit {
    pub label: String,
    pub actions: Vec<Action>,
}

/// Единица работы для очистки.
pub struct CleanUnit {
    pub label: String,
    pub paths: Vec<String>,
    pub recycle: bool,
    pub stop_services: Vec<String>,
}

pub enum Work {
    Apply { label: String, units: Vec<Unit> },
    Clean { units: Vec<CleanUnit> },
}

pub struct Runner {
    progress: Arc<Mutex<Progress>>,
    _handle: JoinHandle<()>,
}

impl Runner {
    pub fn start(preview: bool, work: Work) -> Self {
        let labels: Vec<String> = match &work {
            Work::Apply { units, .. } => units.iter().map(|u| u.label.clone()).collect(),
            Work::Clean { units } => units.iter().map(|u| u.label.clone()).collect(),
        };
        let progress = Arc::new(Mutex::new(Progress::new(preview, labels)));
        let shared = progress.clone();
        let handle = std::thread::spawn(move || run_work(shared, preview, work));
        Self {
            progress,
            _handle: handle,
        }
    }

    pub fn snapshot(&self) -> Progress {
        self.progress.lock().unwrap().clone()
    }

}

fn set(progress: &Arc<Mutex<Progress>>, i: usize, outcome: Outcome) {
    if let Ok(mut p) = progress.lock() {
        if let Some(step) = p.steps.get_mut(i) {
            step.outcome = outcome;
        }
    }
}

fn finish(progress: &Arc<Mutex<Progress>>, summary: String) {
    if let Ok(mut p) = progress.lock() {
        p.finished = true;
        p.summary = summary;
    }
}

fn run_work(progress: Arc<Mutex<Progress>>, preview: bool, work: Work) {
    match work {
        Work::Apply { label, units } => run_apply(&progress, preview, label, units),
        Work::Clean { units } => run_clean(&progress, preview, units),
    }
}

fn run_apply(progress: &Arc<Mutex<Progress>>, preview: bool, label: String, units: Vec<Unit>) {
    let mut journal = sys::Journal::new(label);
    let mut changed = 0usize;
    let mut failed = 0usize;

    for (i, unit) in units.iter().enumerate() {
        set(progress, i, Outcome::Active);
        if preview {
            // Только чтение: сколько действий уже применено.
            let mut applied = 0;
            let mut pending = 0;
            let mut unknown = 0;
            let mut na = 0;
            for a in unit.actions.iter().filter(|a| engine::applies_live(a)) {
                match engine::status(a) {
                    engine::Status::Applied => applied += 1,
                    engine::Status::Pending => pending += 1,
                    engine::Status::Unknown => unknown += 1,
                    engine::Status::NotApplicable => na += 1,
                }
            }
            let total = applied + pending + unknown + na;
            if total > 0 && na == total {
                set(progress, i, Outcome::Good("не застосовно на цій системі".into()));
            } else if pending == 0 && unknown == 0 && total > 0 {
                set(progress, i, Outcome::Good(format!("вже застосовано ({applied}/{total})")));
            } else if pending > 0 {
                changed += 1;
                set(progress, i, Outcome::Warn(format!("буде змінено ({pending} з {total})")));
            } else {
                set(progress, i, Outcome::Warn("перевірка під час застосування".into()));
            }
        } else {
            // Реальное применение.
            let mut errors = Vec::new();
            // Пакеты Appx пункта удаляются одним запуском PowerShell.
            let packages: Vec<String> = unit
                .actions
                .iter()
                .filter(|a| engine::applies_live(a))
                .filter_map(|a| a.appx.clone())
                .collect();
            if let Err(e) = engine::remove_appx(&packages) {
                errors.push(e);
            }
            for a in &unit.actions {
                if !engine::applies_live(a) || a.appx.is_some() {
                    continue;
                }
                match engine::apply(a) {
                    Ok(mut undos) => journal.undos.append(&mut undos),
                    Err(e) => errors.push(e),
                }
            }
            if errors.is_empty() {
                changed += 1;
                set(progress, i, Outcome::Good("готово".into()));
            } else {
                failed += 1;
                set(progress, i, Outcome::Bad(errors.join("; ")));
            }
        }
    }

    if preview {
        finish(progress, format!("Буде змінено пунктів: {changed} з {}", units.len()));
    } else {
        let saved = if journal.undos.is_empty() { None } else { journal.save().ok() };
        let mut summary = match saved {
            Some(path) => format!(
                "Застосовано: {changed}, помилок: {failed}. Журнал відкату: {}",
                path.display()
            ),
            None => format!("Застосовано: {changed}, помилок: {failed}"),
        };
        summary.push_str("\nЧастина змін з'явиться після перезапуску Провідника або перезавантаження.");
        crate::sys::probe::broadcast_settings();
        finish(progress, summary);
    }
}

fn run_clean(progress: &Arc<Mutex<Progress>>, preview: bool, units: Vec<CleanUnit>) {
    let mut total_bytes = 0u64;
    for (i, unit) in units.iter().enumerate() {
        set(progress, i, Outcome::Active);
        if preview {
            let bytes = clean::measure(&unit.paths);
            total_bytes += bytes;
            set(progress, i, Outcome::Warn(format!("знайдено {}", fmt_bytes(bytes))));
        } else {
            let freed = clean::clean(&unit.paths, unit.recycle, &unit.stop_services);
            total_bytes += freed;
            set(progress, i, Outcome::Good(format!("звільнено {}", fmt_bytes(freed))));
        }
    }
    let verb = if preview { "Можна звільнити" } else { "Звільнено" };
    finish(progress, format!("{verb}: {}", fmt_bytes(total_bytes)));
}

/// Байты → человекочитаемый размер.
pub fn fmt_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} ГБ", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0} МБ", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} КБ", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} Б")
    }
}
