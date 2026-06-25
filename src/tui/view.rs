use super::app::App;
use crate::output::bars::usage_bar;
use crate::util::path::display_path;
use crate::util::timing::format_duration;
use crate::util::units::{format_bytes, format_compact_count, format_percent};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{
    Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, TableState, Wrap,
};
use ratatui::Frame;

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    if app.loading {
        draw_loading(frame, app);
        return;
    }

    if app.show_help {
        draw_help(frame);
        return;
    }

    let areas = if app.show_errors {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(8),
                Constraint::Length(8),
                Constraint::Length(1),
            ])
            .split(frame.area())
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(8),
                Constraint::Length(1),
            ])
            .split(frame.area())
    };

    draw_header(frame, app, areas[0]);
    draw_table(frame, app, areas[1]);
    if app.show_errors {
        draw_errors(frame, app, areas[2]);
        draw_footer(frame, areas[3]);
    } else {
        draw_footer(frame, areas[2]);
    }
}

fn draw_loading(frame: &mut Frame<'_>, app: &App) {
    let progress = app.progress.as_ref().map(|progress| progress.snapshot());
    let progress_text = progress
        .map(|progress| {
            format!(
                "Entries: {}\nErrors: {}\nElapsed: {}",
                format_compact_count(progress.entries_seen),
                format_compact_count(progress.errors_seen),
                format_duration(progress.elapsed)
            )
        })
        .unwrap_or_else(|| "Entries: 0\nErrors: 0\nElapsed: 0ms".to_string());
    let action = if app.cancelling {
        "Cancelling..."
    } else {
        "Press q to cancel"
    };
    let message = Paragraph::new(format!(
        "Scanning: {}\n{}\n\n{}",
        display_path(&app.current_path),
        progress_text,
        action
    ))
    .block(Block::default().title("usedu").borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(message, frame.area());
}

fn draw_help(frame: &mut Frame<'_>) {
    let text = [
        "Up / k          Move up",
        "Down / j        Move down",
        "Enter           Open selected directory",
        "Backspace / h   Parent directory",
        "r               Rescan current directory",
        "R               Clear cached result and rescan",
        "s               Toggle sort",
        "e               Toggle error list",
        "?               Toggle help",
        "q               Quit",
    ]
    .join("\n");
    let help = Paragraph::new(text)
        .block(Block::default().title("Help").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(help, frame.area());
}

fn draw_header(frame: &mut Frame<'_>, app: &App, area: ratatui::layout::Rect) {
    let Some(scan) = &app.current_scan else {
        return;
    };
    let title = format!(
        "Path: {}\nUsed: {} | Children: {} | Files: {} | Dirs: {} | Errors: {} | Elapsed: {} | Sort: {}",
        display_path(&scan.root.path),
        format_bytes(scan.root.used_bytes),
        scan.rows.len(),
        format_compact_count(scan.root.file_count),
        format_compact_count(scan.root.dir_count),
        scan.root.errors_count(),
        format_duration(scan.metrics.elapsed),
        app.sort_key.label(),
    );
    frame.render_widget(Paragraph::new(title), area);
}

fn draw_table(frame: &mut Frame<'_>, app: &App, area: ratatui::layout::Rect) {
    let Some(scan) = &app.current_scan else {
        return;
    };
    let rows = app.visible_rows();
    let table_rows = rows.iter().enumerate().map(|(idx, entry)| {
        Row::new(vec![
            Cell::from((idx + 1).to_string()),
            Cell::from(entry.kind_label()),
            Cell::from(entry.name().to_string_lossy().into_owned()),
            Cell::from(format_bytes(entry.used_bytes())),
            Cell::from(format_percent(entry.used_bytes(), scan.root.used_bytes)),
            Cell::from(format!(
                "{} / {}",
                format_compact_count(entry.file_count()),
                format_compact_count(entry.dir_count())
            )),
            Cell::from(usage_bar(entry.used_bytes(), scan.root.used_bytes, 16)),
        ])
    });

    let table = Table::new(
        table_rows,
        [
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Percentage(38),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(18),
        ],
    )
    .header(
        Row::new(vec![
            "#",
            "Kind",
            "Name",
            "Used",
            "Share",
            "Files / Dirs",
            "Visual",
        ])
        .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().borders(Borders::ALL).title("Children"))
    .row_highlight_style(Style::default().bg(Color::DarkGray))
    .highlight_symbol("> ");

    let mut state = TableState::default();
    if !rows.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

fn draw_errors(frame: &mut Frame<'_>, app: &App, area: ratatui::layout::Rect) {
    let Some(scan) = &app.current_scan else {
        return;
    };
    let items: Vec<ListItem> = if scan.root.errors.is_empty() {
        vec![ListItem::new("No errors")]
    } else {
        scan.root
            .errors
            .iter()
            .take(50)
            .map(|error| {
                ListItem::new(format!(
                    "{}: {} ({})",
                    display_path(&error.path),
                    error.message,
                    error.kind
                ))
            })
            .collect()
    };
    frame.render_widget(
        List::new(items).block(Block::default().title("Errors").borders(Borders::ALL)),
        area,
    );
}

fn draw_footer(frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    frame.render_widget(
        Paragraph::new(
            "Enter: open  Backspace: parent  r/R: rescan  s: sort  e: errors  ?: help  q: quit",
        ),
        area,
    );
}
