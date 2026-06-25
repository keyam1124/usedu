use super::view;
use crate::scanner::{
    scan_current_level, CurrentLevelScan, EntrySummary, ScanCancellation, ScanOptions,
    ScanProgress, ScannerError, SortKey,
};
use anyhow::{anyhow, Error, Result};
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::Terminal;
use std::collections::HashMap;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const MAX_CACHE_ENTRIES: usize = 128;

pub fn run(path: PathBuf, options: ScanOptions) -> Result<()> {
    let mut session = TerminalSession::enter()?;
    run_loop(session.terminal_mut(), path, options)
}

fn run_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    path: PathBuf,
    options: ScanOptions,
) -> Result<()> {
    let mut app = App::new(path, options);
    if !load_current(terminal, &mut app, true)? {
        return Ok(());
    }

    loop {
        terminal.draw(|frame| view::draw(frame, &app))?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };

        match key.code {
            KeyCode::Char('q') => break,
            KeyCode::Char('?') => app.show_help = !app.show_help,
            KeyCode::Char('e') => app.show_errors = !app.show_errors,
            KeyCode::Char('s') => app.sort_key = app.sort_key.next(),
            KeyCode::Char('r') => {
                if !load_current(terminal, &mut app, false)? {
                    break;
                }
            }
            KeyCode::Char('R') => {
                app.clear_relevant_cache();
                if !load_current(terminal, &mut app, false)? {
                    break;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
            KeyCode::PageDown => app.move_selection(10),
            KeyCode::PageUp => app.move_selection(-10),
            KeyCode::Enter => {
                if let Some(entry) = app.selected_entry() {
                    if entry.is_dir() {
                        app.current_path = entry.path().to_path_buf();
                        app.selected = 0;
                        app.show_errors = false;
                        if !load_current(terminal, &mut app, true)? {
                            break;
                        }
                    }
                }
            }
            KeyCode::Backspace | KeyCode::Char('h') => {
                if let Some(parent) = app.current_path.parent() {
                    app.current_path = parent.to_path_buf();
                    app.selected = 0;
                    app.show_errors = false;
                    if !load_current(terminal, &mut app, true)? {
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn load_current<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    use_cache: bool,
) -> Result<bool> {
    if use_cache {
        if let Some(scan) = app.cache.get(&app.current_path).cloned() {
            app.current_scan = Some(scan);
            app.clamp_selection();
            return Ok(true);
        }
    }

    let progress = ScanProgress::new();
    let cancellation = ScanCancellation::default();
    let mut options = app.options.clone();
    options.progress = Some(progress.clone());
    options.cancellation = Some(cancellation.clone());
    options.retained_tree_depth = 1;
    let path = app.current_path.clone();
    let (sender, receiver) = mpsc::channel();

    app.loading = true;
    app.cancelling = false;
    app.progress = Some(progress);
    app.current_scan = None;
    thread::spawn(move || {
        let result = scan_current_level(path, &options);
        let _ = sender.send(result);
    });

    loop {
        terminal.draw(|frame| view::draw(frame, app))?;

        match receiver.try_recv() {
            Ok(Ok(scan)) => {
                app.current_path = scan.root.path.clone();
                app.insert_cache(app.current_path.clone(), scan.clone());
                app.current_scan = Some(scan);
                app.loading = false;
                app.cancelling = false;
                app.progress = None;
                app.clamp_selection();
                return Ok(true);
            }
            Ok(Err(error)) if is_cancelled_error(&error) => {
                app.loading = false;
                app.cancelling = false;
                app.progress = None;
                return Ok(false);
            }
            Ok(Err(error)) => {
                app.loading = false;
                app.cancelling = false;
                app.progress = None;
                return Err(error);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                app.loading = false;
                app.cancelling = false;
                app.progress = None;
                return Err(anyhow!("scanner worker disconnected"));
            }
        }

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.code == KeyCode::Char('q') {
            app.cancelling = true;
            cancellation.cancel();
        }
    }
}

pub struct App {
    pub current_path: PathBuf,
    pub options: ScanOptions,
    pub cache: HashMap<PathBuf, CurrentLevelScan>,
    pub current_scan: Option<CurrentLevelScan>,
    pub selected: usize,
    pub sort_key: SortKey,
    pub show_errors: bool,
    pub show_help: bool,
    pub loading: bool,
    pub cancelling: bool,
    pub progress: Option<ScanProgress>,
}

impl App {
    fn new(path: PathBuf, options: ScanOptions) -> Self {
        Self {
            current_path: path,
            options,
            cache: HashMap::new(),
            current_scan: None,
            selected: 0,
            sort_key: SortKey::Used,
            show_errors: false,
            show_help: false,
            loading: false,
            cancelling: false,
            progress: None,
        }
    }

    pub fn visible_rows(&self) -> Vec<&EntrySummary> {
        let Some(scan) = &self.current_scan else {
            return Vec::new();
        };
        let mut rows: Vec<&EntrySummary> = scan.rows.iter().collect();
        rows.sort_by(|left, right| match self.sort_key {
            SortKey::Used => right
                .used_bytes()
                .cmp(&left.used_bytes())
                .then_with(|| entry_name(left).cmp(&entry_name(right))),
            SortKey::Name => entry_name(left)
                .cmp(&entry_name(right))
                .then_with(|| right.used_bytes().cmp(&left.used_bytes())),
            SortKey::Files => right
                .file_count()
                .cmp(&left.file_count())
                .then_with(|| right.used_bytes().cmp(&left.used_bytes())),
            SortKey::Dirs => right
                .dir_count()
                .cmp(&left.dir_count())
                .then_with(|| right.used_bytes().cmp(&left.used_bytes())),
        });
        rows
    }

    fn selected_entry(&self) -> Option<&EntrySummary> {
        self.visible_rows().get(self.selected).copied()
    }

    fn move_selection(&mut self, delta: isize) {
        let row_count = self.visible_rows().len();
        if row_count == 0 {
            self.selected = 0;
            return;
        }

        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, row_count.saturating_sub(1) as isize) as usize;
    }

    fn clamp_selection(&mut self) {
        let row_count = self.visible_rows().len();
        if row_count == 0 {
            self.selected = 0;
        } else if self.selected >= row_count {
            self.selected = row_count - 1;
        }
    }

    fn clear_relevant_cache(&mut self) {
        let current = self.current_path.clone();
        self.cache.retain(|path, _| !path.starts_with(&current));
    }

    fn insert_cache(&mut self, path: PathBuf, scan: CurrentLevelScan) {
        if self.cache.len() >= MAX_CACHE_ENTRIES && !self.cache.contains_key(&path) {
            self.cache.clear();
        }
        self.cache.insert(path, scan);
    }
}

fn entry_name(entry: &EntrySummary) -> String {
    crate::util::path::display_os_str_human(entry.name())
}

fn is_cancelled_error(error: &Error) -> bool {
    error
        .downcast_ref::<ScannerError>()
        .is_some_and(|error| matches!(error, ScannerError::Cancelled))
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = disable_raw_mode();
                Err(error.into())
            }
        }
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}
