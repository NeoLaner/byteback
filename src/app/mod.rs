//! The full-screen dashboard: a small state machine driving target selection,
//! scanning, the results review, and the delete confirmation.

mod view;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use ratatui::widgets::ListState;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::config::Config;
use crate::delete::{Disposal, Outcome};
use crate::report::ScanReport;
use crate::scan::{self, ScanEvent};
use crate::target::TargetName;

/// Which screen the dashboard is showing.
enum Mode {
    /// Pick target names and the scan directory.
    Select,
    /// A background scan is running.
    Scanning,
    /// Review found dirs and opt some out.
    Review,
    /// Show the freed-space summary.
    Done,
}

/// A pop-up layered over the current screen.
enum Overlay {
    None,
    AddName(Input),
    ChangePath(Input),
    ConfirmDelete,
}

/// A target name with its checked state in the selection panel.
struct Selectable {
    name: TargetName,
    enabled: bool,
}

/// A row in the results panel: a category header or one directory (a flat index
/// into [`ScanReport::dirs`]). Headers are skipped during navigation.
enum Row {
    Header(usize),
    Dir(usize),
}

pub struct App {
    config: Config,
    root: PathBuf,
    disposal: Disposal,

    mode: Mode,
    overlay: Overlay,
    quit: bool,

    targets: Vec<Selectable>,
    targets_state: ListState,

    scan_rx: Option<Receiver<ScanEvent>>,
    found_total: Option<usize>,
    measured: usize,

    report: ScanReport,
    rows: Vec<Row>,
    results_state: ListState,

    outcome: Option<Outcome>,
}

impl App {
    pub fn new(config: Config, root: PathBuf, disposal: Disposal) -> Self {
        let enabled = config.initial_enabled();
        let targets: Vec<Selectable> = config
            .available()
            .into_iter()
            .map(|name| Selectable {
                enabled: enabled.contains(&name),
                name,
            })
            .collect();

        let mut targets_state = ListState::default();
        if !targets.is_empty() {
            targets_state.select(Some(0));
        }

        Self {
            config,
            root,
            disposal,
            mode: Mode::Select,
            overlay: Overlay::None,
            quit: false,
            targets,
            targets_state,
            scan_rx: None,
            found_total: None,
            measured: 0,
            report: ScanReport::default(),
            rows: Vec::new(),
            results_state: ListState::default(),
            outcome: None,
        }
    }

    /// Run the event loop until the user quits. Returns the deletion outcome, if
    /// any deletion happened.
    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<Option<Outcome>> {
        while !self.quit {
            terminal.draw(|frame| view::draw(&mut self, frame))?;
            self.pump_scan();
            if event::poll(Duration::from_millis(100))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.on_key(key);
            }
        }
        Ok(self.outcome)
    }

    /// Drain any progress from the background scan without blocking.
    fn pump_scan(&mut self) {
        let events: Vec<ScanEvent> = match self.scan_rx.as_ref() {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };
        for event in events {
            match event {
                ScanEvent::Found(total) => self.found_total = Some(total),
                ScanEvent::Measured => self.measured += 1,
                ScanEvent::Done(report) => self.finish_scan(report),
            }
        }
    }

    fn finish_scan(&mut self, report: ScanReport) {
        self.report = report;
        self.rebuild_rows();
        self.scan_rx = None;
        self.mode = Mode::Review;
    }

    fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        for (category_index, category) in self.report.categories.iter().enumerate() {
            rows.push(Row::Header(category_index));
            for dir_index in category.start..category.start + category.len {
                rows.push(Row::Dir(dir_index));
            }
        }
        self.rows = rows;
        let first_dir = self.rows.iter().position(|row| matches!(row, Row::Dir(_)));
        self.results_state.select(first_dir);
    }

    fn on_key(&mut self, key: KeyEvent) {
        // Overlays capture all input while open.
        match self.overlay {
            Overlay::AddName(_) | Overlay::ChangePath(_) => return self.on_input_key(key),
            Overlay::ConfirmDelete => return self.on_confirm_key(key),
            Overlay::None => {}
        }

        // Any key dismisses the final summary.
        if matches!(self.mode, Mode::Done) {
            self.quit = true;
            return;
        }

        if matches!(key.code, KeyCode::Char('q')) || is_ctrl_c(key) {
            self.quit = true;
            return;
        }

        match self.mode {
            Mode::Select => self.on_select_key(key),
            Mode::Scanning => {
                if matches!(key.code, KeyCode::Esc) {
                    self.scan_rx = None;
                    self.mode = Mode::Select;
                }
            }
            Mode::Review => self.on_review_key(key),
            Mode::Done => {}
        }
    }

    fn on_select_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                move_selection(&mut self.targets_state, self.targets.len(), -1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                move_selection(&mut self.targets_state, self.targets.len(), 1)
            }
            KeyCode::Char(' ') => self.toggle_target(),
            KeyCode::Char('a') => self.overlay = Overlay::AddName(Input::default()),
            KeyCode::Char('r') | KeyCode::Delete => self.remove_target(),
            KeyCode::Char('c') => {
                let current = self.root.display().to_string();
                self.overlay = Overlay::ChangePath(Input::new(current));
            }
            KeyCode::Enter | KeyCode::Char('s') => self.start_scan(),
            _ => {}
        }
    }

    fn on_review_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_result(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_result(1),
            KeyCode::Char(' ') => {
                if let Some(index) = self.selected_dir() {
                    self.report.toggle(index);
                }
            }
            KeyCode::Char('a') => self.report.set_all(true),
            KeyCode::Char('n') => self.report.set_all(false),
            KeyCode::Char('t') => self.disposal = Disposal::Trash,
            KeyCode::Char('p') => self.disposal = Disposal::Permanent,
            KeyCode::Char('d') if self.report.selected_count() > 0 => {
                self.overlay = Overlay::ConfirmDelete;
            }
            KeyCode::Esc | KeyCode::Char('b') => self.mode = Mode::Select,
            _ => {}
        }
    }

    fn on_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.overlay = Overlay::None,
            KeyCode::Enter => self.commit_overlay(),
            _ => {
                if let Overlay::AddName(input) | Overlay::ChangePath(input) = &mut self.overlay {
                    input.handle_event(&Event::Key(key));
                }
            }
        }
    }

    fn on_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
                self.overlay = Overlay::None;
                self.perform_delete();
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => self.overlay = Overlay::None,
            _ => {}
        }
    }

    fn commit_overlay(&mut self) {
        match std::mem::replace(&mut self.overlay, Overlay::None) {
            Overlay::AddName(input) => {
                let value = input.value().trim().to_owned();
                if !value.is_empty() {
                    let name = TargetName::from(value);
                    self.config.add_custom(name.clone());
                    let _ = self.config.save();
                    self.reload_targets();
                    if let Some(target) = self.targets.iter_mut().find(|t| t.name == name) {
                        target.enabled = true;
                    }
                }
            }
            Overlay::ChangePath(input) => {
                let value = input.value().trim().to_owned();
                let path = PathBuf::from(value);
                if path.is_dir() {
                    self.root = path;
                }
            }
            Overlay::ConfirmDelete | Overlay::None => {}
        }
    }

    fn toggle_target(&mut self) {
        if let Some(index) = self.targets_state.selected()
            && let Some(target) = self.targets.get_mut(index)
        {
            target.enabled = !target.enabled;
        }
    }

    fn remove_target(&mut self) {
        let Some(index) = self.targets_state.selected() else {
            return;
        };
        let Some(target) = self.targets.get(index) else {
            return;
        };
        let name = target.name.clone();
        self.config.remove(&name);
        let _ = self.config.save();
        self.reload_targets();
    }

    fn reload_targets(&mut self) {
        let enabled: HashSet<TargetName> = self
            .targets
            .iter()
            .filter(|t| t.enabled)
            .map(|t| t.name.clone())
            .collect();
        self.targets = self
            .config
            .available()
            .into_iter()
            .map(|name| Selectable {
                enabled: enabled.contains(&name),
                name,
            })
            .collect();

        match self.targets.len() {
            0 => self.targets_state.select(None),
            len => {
                let clamped = self.targets_state.selected().unwrap_or(0).min(len - 1);
                self.targets_state.select(Some(clamped));
            }
        }
    }

    fn start_scan(&mut self) {
        let enabled: Vec<TargetName> = self
            .targets
            .iter()
            .filter(|t| t.enabled)
            .map(|t| t.name.clone())
            .collect();
        if enabled.is_empty() {
            return;
        }
        self.config.set_enabled(enabled.clone());
        let _ = self.config.save();

        self.scan_rx = Some(scan::spawn(self.root.clone(), enabled));
        self.found_total = None;
        self.measured = 0;
        self.mode = Mode::Scanning;
    }

    fn perform_delete(&mut self) {
        let outcome = {
            let selected = self.report.selected();
            self.disposal.dispose(&selected, &self.root)
        };
        self.outcome = Some(outcome);
        self.mode = Mode::Done;
    }

    fn move_result(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len() as isize;
        let mut index = self.results_state.selected().unwrap_or(0) as isize;
        loop {
            index += delta;
            if index < 0 || index >= len {
                return;
            }
            if matches!(self.rows[index as usize], Row::Dir(_)) {
                self.results_state.select(Some(index as usize));
                return;
            }
        }
    }

    fn selected_dir(&self) -> Option<usize> {
        match self.rows.get(self.results_state.selected()?)? {
            Row::Dir(index) => Some(*index),
            Row::Header(_) => None,
        }
    }
}

/// Clamp-move a list selection by `delta`, staying within `[0, len)`.
fn move_selection(state: &mut ListState, len: usize, delta: isize) {
    if len == 0 {
        return;
    }
    let current = state.selected().unwrap_or(0) as isize;
    let next = (current + delta).clamp(0, len as isize - 1);
    state.select(Some(next as usize));
}

fn is_ctrl_c(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c'))
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::delete::Outcome;
    use crate::report::FoundDir;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn app() -> App {
        App::new(Config::default(), PathBuf::from("/tmp"), Disposal::Trash)
    }

    fn sample_report() -> ScanReport {
        ScanReport::from_dirs(vec![
            FoundDir {
                path: PathBuf::from("/tmp/a/node_modules"),
                target: TargetName::from("node_modules"),
                size: 1234,
                file_count: 12,
                selected: true,
            },
            FoundDir {
                path: PathBuf::from("/tmp/b/.next"),
                target: TargetName::from(".next"),
                size: 5678,
                file_count: 34,
                selected: true,
            },
        ])
    }

    fn render(app: &mut App, width: u16, height: u16) {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| view::draw(app, frame)).unwrap();
    }

    #[test]
    fn space_toggles_the_highlighted_target() {
        let mut app = app();
        assert!(app.targets[0].enabled);
        app.on_key(press(KeyCode::Char(' ')));
        assert!(!app.targets[0].enabled);
    }

    #[test]
    fn removing_hides_the_highlighted_target() {
        let mut app = app();
        let first = app.targets[0].name.clone();
        app.on_key(press(KeyCode::Char('r')));
        assert!(!app.targets.iter().any(|t| t.name == first));
    }

    #[test]
    fn adding_a_custom_name_inserts_and_enables_it() {
        let mut app = app();
        let before = app.targets.len();

        app.on_key(press(KeyCode::Char('a')));
        for ch in "my-cache".chars() {
            app.on_key(press(KeyCode::Char(ch)));
        }
        app.on_key(press(KeyCode::Enter));

        assert_eq!(app.targets.len(), before + 1);
        let added = app
            .targets
            .iter()
            .find(|t| t.name == TargetName::from("my-cache"))
            .expect("custom name present");
        assert!(added.enabled);
        assert!(matches!(app.overlay, Overlay::None));
    }

    #[test]
    fn t_and_p_switch_the_disposal_mode_in_review() {
        let mut app = app();
        app.mode = Mode::Review;
        app.on_key(press(KeyCode::Char('p')));
        assert_eq!(app.disposal, Disposal::Permanent);
        app.on_key(press(KeyCode::Char('t')));
        assert_eq!(app.disposal, Disposal::Trash);
    }

    #[test]
    fn renders_every_mode_and_overlay_without_panicking() {
        let mut app = app();
        render(&mut app, 80, 24); // Select

        app.mode = Mode::Scanning;
        app.found_total = Some(3);
        app.measured = 1;
        render(&mut app, 80, 24);

        app.report = sample_report();
        app.rebuild_rows();
        app.mode = Mode::Review;
        render(&mut app, 80, 24);

        app.overlay = Overlay::ConfirmDelete;
        render(&mut app, 80, 24);

        app.mode = Mode::Select;
        app.overlay = Overlay::AddName(Input::new("foo".to_owned()));
        render(&mut app, 80, 24);

        app.overlay = Overlay::None;
        app.mode = Mode::Done;
        app.outcome = Some(Outcome {
            freed_bytes: 6912,
            removed: 2,
            failures: Vec::new(),
        });
        render(&mut app, 80, 24);
    }

    #[test]
    fn renders_in_a_tiny_viewport_without_panicking() {
        // Guards the modal centring math against underflow on small screens.
        let mut app = app();
        app.overlay = Overlay::AddName(Input::default());
        render(&mut app, 8, 4);

        app.report = sample_report();
        app.rebuild_rows();
        app.mode = Mode::Review;
        app.overlay = Overlay::ConfirmDelete;
        render(&mut app, 6, 3);
    }
}
