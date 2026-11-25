//! Фоновое выполнение задач (применение твиков, очистка) в отдельном потоке
//! с живым прогрессом. Два режима: предпросмотр (только чтение состояния) и
//! реальное применение (с журналом отката).

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::catalog::Action;
use crate::i18n::Lang;
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
    /// `guard`: первый шаг — проверка безопасности (точка восстановления). Если он
    /// не удался, остальное не применяется.
    Apply { label: String, units: Vec<Unit>, guard: bool },
    Clean { units: Vec<CleanUnit> },
}

pub struct Runner {
    progress: Arc<Mutex<Progress>>,
    _handle: JoinHandle<()>,
}

impl Runner {
    pub fn start(preview: bool, work: Work, lang: Lang) -> Self {
        let labels: Vec<String> = match &work {
            Work::Apply { units, .. } => units.iter().map(|u| u.label.clone()).collect(),
            Work::Clean { units } => units.iter().map(|u| u.label.clone()).collect(),
        };
        let progress = Arc::new(Mutex::new(Progress::new(preview, labels)));
        let shared = progress.clone();
        let handle = std::thread::spawn(move || run_work(shared, preview, work, lang));
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

fn run_work(progress: Arc<Mutex<Progress>>, preview: bool, work: Work, l: Lang) {
    match work {
        Work::Apply { label, units, guard } => run_apply(&progress, preview, label, units, guard, l),
        Work::Clean { units } => run_clean(&progress, preview, units, l),
    }
}

fn run_apply(progress: &Arc<Mutex<Progress>>, preview: bool, label: String, units: Vec<Unit>, guard: bool, l: Lang) {
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
                set(progress, i, Outcome::Good(l.tr("не застосовно на цій системі", "not applicable on this system").into()));
            } else if pending == 0 && unknown == 0 && total > 0 {
                set(progress, i, Outcome::Good(match l {
                    Lang::Uk => format!("вже застосовано ({applied}/{total})"),
                    Lang::En => format!("already applied ({applied}/{total})"),
                }));
            } else if pending > 0 {
                changed += 1;
                set(progress, i, Outcome::Warn(match l {
                    Lang::Uk => format!("буде змінено ({pending} з {total})"),
                    Lang::En => format!("will change ({pending} of {total})"),
                }));
            } else {
                set(progress, i, Outcome::Warn(l.tr("перевірка під час застосування", "checked while applying").into()));
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
                set(progress, i, Outcome::Good(l.tr("готово", "done").into()));
            } else {
                failed += 1;
                set(progress, i, Outcome::Bad(errors.join("; ")));
                if guard && i == 0 {
                    // Без точки восстановления систему не трогаем.
                    for j in 1..units.len() {
                        set(progress, j, Outcome::Warn(l.tr("не застосовано", "not applied").into()));
                    }
                    finish(
                        progress,
                        l.tr(
                            "Точку відновлення не створено, тому нічого не змінено. Увімкни «Захист системи» для диска C: або зніми галочку точки відновлення.",
                            "No restore point was created, so nothing was changed. Turn on System Protection for drive C: or untick the restore point option.",
                        )
                        .into(),
                    );
                    return;
                }
            }
        }
    }

    if preview {
        finish(
            progress,
            match l {
                Lang::Uk => format!("Буде змінено пунктів: {changed} з {}", units.len()),
                Lang::En => format!("Items that will change: {changed} of {}", units.len()),
            },
        );
    } else {
        let saved = if journal.undos.is_empty() { None } else { journal.save().ok() };
        let mut summary = match saved {
            Some(path) => match l {
                Lang::Uk => format!("Застосовано: {changed}, помилок: {failed}. Журнал відкату: {}", path.display()),
                Lang::En => format!("Applied: {changed}, errors: {failed}. Undo journal: {}", path.display()),
            },
            None => match l {
                Lang::Uk => format!("Застосовано: {changed}, помилок: {failed}"),
                Lang::En => format!("Applied: {changed}, errors: {failed}"),
            },
        };
        summary.push_str(l.tr(
            "\nЧастина змін з'явиться після перезапуску Провідника або перезавантаження.",
            "\nSome changes appear after restarting Explorer or the PC.",
        ));
        crate::sys::probe::broadcast_settings();
        finish(progress, summary);
    }
}

fn run_clean(progress: &Arc<Mutex<Progress>>, preview: bool, units: Vec<CleanUnit>, l: Lang) {
    let mut total_bytes = 0u64;
    for (i, unit) in units.iter().enumerate() {
        set(progress, i, Outcome::Active);
        if preview {
            let bytes = clean::measure(&unit.paths);
            total_bytes += bytes;
            set(progress, i, Outcome::Warn(format!("{} {}", l.tr("знайдено", "found"), fmt_bytes(bytes, l))));
        } else {
            let freed = clean::clean(&unit.paths, unit.recycle, &unit.stop_services);
            total_bytes += freed;
            set(progress, i, Outcome::Good(format!("{} {}", l.tr("звільнено", "freed"), fmt_bytes(freed, l))));
        }
    }
    let verb = if preview { l.tr("Можна звільнити", "Can be freed") } else { l.tr("Звільнено", "Freed") };
    finish(progress, format!("{verb}: {}", fmt_bytes(total_bytes, l)));
}

/// Байты → человекочитаемый размер.
pub fn fmt_bytes(bytes: u64, l: Lang) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} {}", bytes as f64 / GB as f64, l.tr("ГБ", "GB"))
    } else if bytes >= MB {
        format!("{:.0} {}", bytes as f64 / MB as f64, l.tr("МБ", "MB"))
    } else if bytes >= KB {
        format!("{:.0} {}", bytes as f64 / KB as f64, l.tr("КБ", "KB"))
    } else {
        format!("{bytes} {}", l.tr("Б", "B"))
    }
}

