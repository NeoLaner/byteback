//! Rendering for the dashboard. Pure draw functions: they read [`App`] state and
//! paint widgets, never mutating anything but the list scroll state.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph, Wrap};
use tui_input::Input;

use super::{App, Mode, Overlay, Row};
use crate::delete::Disposal;
use crate::report::human_size;

pub fn draw(app: &mut App, frame: &mut Frame) {
    let full = frame.area();
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .areas(full);

    draw_header(app, frame, header);
    match app.mode {
        Mode::Select => draw_select(app, frame, body),
        Mode::Scanning => draw_scanning(app, frame, body),
        Mode::Review => draw_review(app, frame, body),
        Mode::Done => draw_done(app, frame, body),
    }
    draw_footer(app, frame, footer);
    draw_overlay(app, frame, full);
}

fn draw_header(app: &App, frame: &mut Frame, area: Rect) {
    let status = match app.mode {
        Mode::Select => format!(
            "{} target(s) selected",
            app.targets.iter().filter(|t| t.enabled).count()
        ),
        Mode::Scanning => match app.found_total {
            Some(total) => format!("scanning… measured {}/{}", app.measured, total),
            None => "scanning… discovering directories".to_owned(),
        },
        Mode::Review => format!(
            "{} of {} dirs · {} of {} · {} files",
            app.report.selected_count(),
            app.report.dir_count(),
            human_size(app.report.selected_size()),
            human_size(app.report.total_size()),
            thousands(app.report.total_files()),
        ),
        Mode::Done => "done".to_owned(),
    };

    let line = Line::from(vec![
        Span::styled(
            app.root.display().to_string(),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("  ·  "),
        Span::styled(status, dim()),
    ]);
    let block = Block::bordered().title(Span::styled(
        " byteback ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn draw_select(app: &mut App, frame: &mut Frame, area: Rect) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(area);

    let items: Vec<ListItem> = app
        .targets
        .iter()
        .map(|target| {
            let (mark, mark_style) = checkbox(target.enabled);
            ListItem::new(Line::from(vec![
                Span::styled(mark, mark_style),
                Span::raw(target.name.to_string()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::bordered().title(" Targets "))
        .highlight_style(highlight())
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, left, &mut app.targets_state);

    let help = Paragraph::new(vec![
        Line::from("Pick the directory names to sweep."),
        Line::from(""),
        Line::from(vec![
            Span::styled("space", key()),
            Span::raw("  toggle on/off"),
        ]),
        Line::from(vec![
            Span::styled("a    ", key()),
            Span::raw("  add a custom name"),
        ]),
        Line::from(vec![
            Span::styled("r    ", key()),
            Span::raw("  remove the highlighted name"),
        ]),
        Line::from(vec![
            Span::styled("c    ", key()),
            Span::raw("  change scan directory"),
        ]),
        Line::from(vec![
            Span::styled("enter", key()),
            Span::raw("  scan for them"),
        ]),
    ])
    .block(Block::bordered().title(" How it works "))
    .wrap(Wrap { trim: true });
    frame.render_widget(help, right);
}

fn draw_scanning(app: &App, frame: &mut Frame, area: Rect) {
    let lines = match app.found_total {
        Some(total) if total > 0 => vec![
            Line::from(format!("Measuring {total} directories…")),
            Line::from(format!("{} done", app.measured)),
        ],
        Some(_) => vec![Line::from("No matching directories found.")],
        None => vec![Line::from("Searching…")],
    };
    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(Block::bordered().title(" Scanning "));
    frame.render_widget(paragraph, area);
}

fn draw_review(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.report.is_empty() {
        let paragraph = Paragraph::new(vec![
            Line::from("No matching directories found."),
            Line::from(""),
            Line::from(Span::styled("press b to choose other targets", dim())),
        ])
        .alignment(Alignment::Center)
        .block(Block::bordered().title(" Cleanable directories "));
        frame.render_widget(paragraph, area);
        return;
    }

    let root = app.root.clone();
    let mut items: Vec<ListItem> = Vec::with_capacity(app.rows.len());
    for row in &app.rows {
        let item = match row {
            Row::Header(category_index) => {
                let category = &app.report.categories[*category_index];
                let dirs = app.report.category_dirs(category);
                let size: u64 = dirs.iter().map(|d| d.size).sum();
                let files: u64 = dirs.iter().map(|d| d.file_count).sum();
                let text = format!(
                    "{}  —  {} dir(s) · {} · {} files",
                    category.target,
                    dirs.len(),
                    human_size(size),
                    thousands(files),
                );
                ListItem::new(Line::from(Span::styled(
                    text,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )))
            }
            Row::Dir(dir_index) => {
                let dir = &app.report.dirs[*dir_index];
                let rel = dir.path.strip_prefix(&root).unwrap_or(&dir.path);
                let (mark, mark_style) = checkbox(dir.selected);
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(mark, mark_style),
                    Span::raw(format!("./{}", rel.display())),
                    Span::raw("  "),
                    Span::styled(human_size(dir.size), Style::default().fg(Color::Cyan)),
                    Span::styled(format!("  {} files", thousands(dir.file_count)), dim()),
                ]))
            }
        };
        items.push(item);
    }

    let list = List::new(items)
        .block(Block::bordered().title(" Cleanable directories "))
        .highlight_style(highlight());
    frame.render_stateful_widget(list, area, &mut app.results_state);
}

fn draw_done(app: &App, frame: &mut Frame, area: Rect) {
    let mut lines = Vec::new();
    match &app.outcome {
        Some(outcome) => {
            lines.push(Line::from(Span::styled(
                format!(
                    "Reclaimed {} from {} {}.",
                    human_size(outcome.freed_bytes),
                    outcome.removed,
                    plural(outcome.removed, "directory", "directories"),
                ),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(vec![
                Span::raw("Mode: "),
                Span::raw(app.disposal.label()),
            ]));
            if !outcome.failures.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("{} could not be removed:", outcome.failures.len()),
                    Style::default().fg(Color::Red),
                )));
                for (path, error) in outcome.failures.iter().take(8) {
                    lines.push(Line::from(format!("  {} — {error}", path.display())));
                }
            }
        }
        None => lines.push(Line::from("Nothing was deleted.")),
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("press any key to exit", dim())));

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(" Done "))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_footer(app: &App, frame: &mut Frame, area: Rect) {
    let line = match app.mode {
        Mode::Select => {
            Line::from("↑↓ move · space toggle · a add · r remove · c dir · enter scan · q quit")
        }
        Mode::Scanning => Line::from("esc cancel"),
        Mode::Review => {
            let (trash_style, permanent_style) = match app.disposal {
                Disposal::Trash => (
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                    dim(),
                ),
                Disposal::Permanent => (
                    dim(),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            };
            Line::from(vec![
                Span::raw("↑↓ move · space toggle · a all · n none · d delete · b back   "),
                Span::styled("[t] trash", trash_style),
                Span::raw("  "),
                Span::styled("[p] permanent", permanent_style),
            ])
        }
        Mode::Done => Line::from("press any key to exit"),
    };
    frame.render_widget(Paragraph::new(line).block(Block::bordered()), area);
}

fn draw_overlay(app: &App, frame: &mut Frame, area: Rect) {
    match &app.overlay {
        Overlay::None => {}
        Overlay::AddName(input) => draw_input_modal(
            frame,
            area,
            " Add a custom name ",
            "Directory name to add:",
            input,
        ),
        Overlay::ChangePath(input) => draw_input_modal(
            frame,
            area,
            " Change scan directory ",
            "Directory to scan:",
            input,
        ),
        Overlay::ConfirmDelete => draw_confirm_modal(app, frame, area),
    }
}

fn draw_input_modal(frame: &mut Frame, area: Rect, title: &str, prompt: &str, input: &Input) {
    let modal = centered_rect(60, 7, area);
    frame.render_widget(Clear, modal);
    let lines = vec![
        Line::from(prompt),
        Line::from(""),
        Line::from(Span::styled(
            format!("> {}", input.value()),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(Span::styled("enter confirm · esc cancel", dim())),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(title)),
        modal,
    );

    // Border (1) + the "> " prefix (2); the input line is the 3rd inner row.
    let cursor_x = modal.x + 1 + 2 + input.visual_cursor() as u16;
    let cursor_y = modal.y + 1 + 2;
    frame.set_cursor_position((cursor_x, cursor_y));
}

fn draw_confirm_modal(app: &App, frame: &mut Frame, area: Rect) {
    let modal = centered_rect(60, 8, area);
    frame.render_widget(Clear, modal);

    let count = app.report.selected_count();
    let (verb, verb_style) = match app.disposal {
        Disposal::Trash => ("move to trash", Style::default().fg(Color::Green)),
        Disposal::Permanent => (
            "permanently delete",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    };
    let lines = vec![
        Line::from(vec![Span::raw("About to "), Span::styled(verb, verb_style)]),
        Line::from(format!(
            "{count} {} · {} to free",
            plural(count, "directory", "directories"),
            human_size(app.report.selected_size()),
        )),
        Line::from(""),
        Line::from(Span::styled("y confirm · n cancel", dim())),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(" Confirm "))
            .wrap(Wrap { trim: true }),
        modal,
    );
}

/// A box of `width_percent` of `area` and a fixed `height`, centred in `area`.
fn centered_rect(width_percent: u16, height: u16, area: Rect) -> Rect {
    let width = area.width * width_percent / 100;
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn checkbox(checked: bool) -> (&'static str, Style) {
    if checked {
        ("[x] ", Style::default().fg(Color::Green))
    } else {
        ("[ ] ", dim())
    }
}

fn highlight() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

fn key() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

/// Group digits in threes, e.g. `1234567` → `1,234,567`.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let len = digits.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
