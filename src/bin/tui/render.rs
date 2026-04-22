//! All `draw_*` rendering functions for the TUI.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Clear, Dataset, GraphType, List, ListItem, ListState,
        Paragraph,
    },
    Frame,
};

use super::app::{App, AppMode};
use super::helpers::{
    format_voltage, smooth_signal, COLORS, DIM_COLORS, SMOOTH_WINDOW, SPINNER, WINDOW_SECS,
};

// ── Top-level draw ───────────────────────────────────────────────────────────

pub(crate) fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let root = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .split(area);

    draw_header(frame, root[0], app);
    draw_charts(frame, root[1], app);
    draw_footer(frame, root[2], app);

    if app.show_picker {
        draw_device_picker(frame, area, app);
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn spinner_str() -> &'static str {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    SPINNER[(ms / 100) as usize % SPINNER.len()]
}

#[inline]
fn sep<'a>() -> Span<'a> {
    Span::styled(" \u{2502} ", Style::default().fg(Color::DarkGray))
}

#[inline]
fn key(s: &str) -> Span<'_> {
    Span::styled(
        s,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
}

// ── Header ───────────────────────────────────────────────────────────────────

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let (label, color) = match &app.mode {
        AppMode::Scanning => (format!("{} Scanning...", spinner_str()), Color::Yellow),
        AppMode::Connecting(serial) => (
            format!("{} Connecting to {}...", spinner_str(), serial),
            Color::Yellow,
        ),
        AppMode::Connected { serial } => {
            (format!("\u{25cf} {}", serial), Color::Green)
        }
        AppMode::Simulated => ("\u{25c6} Simulated".to_owned(), Color::Cyan),
        AppMode::NoDevices => (
            format!("{} No amplifiers found - retrying...", spinner_str()),
            Color::Yellow,
        ),
        AppMode::Disconnected => {
            let reason = app
                .last_error
                .as_deref()
                .map(|e| format!(" ({e})"))
                .unwrap_or_default();
            (
                format!("{} Disconnected{reason} - retrying...", spinner_str()),
                Color::Red,
            )
        }
    };

    let bat = app
        .battery_level
        .map(|b| format!("Bat {}%", b))
        .unwrap_or_else(|| "Bat N/A".into());

    let rate = format!("{:.1} pkt/s", app.pkt_rate());
    let scale = format!("{}", format_voltage(app.y_range()));
    let total = format!("{}K smp", app.total_samples / 1_000);

    let line = Line::from(vec![
        Span::styled(
            " ANT Neuro EEG Monitor ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        sep(),
        Span::styled(
            label,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        sep(),
        Span::styled(bat, Style::default().fg(Color::White)),
        sep(),
        Span::styled(rate, Style::default().fg(Color::White)),
        sep(),
        Span::styled(
            format!("+/-{}", scale),
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        ),
        sep(),
        Span::styled(total, Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
    ]);

    frame.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

// ── Charts ───────────────────────────────────────────────────────────────────

fn draw_charts(frame: &mut Frame, area: Rect, app: &App) {
    let n = app.display_channels();
    if n == 0 {
        return;
    }
    let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Ratio(1, n as u32)).collect();
    let rows = Layout::vertical(constraints).split(area);

    let y_range = app.y_range();
    let hz = app.sample_rate;

    for ch in 0..n {
        if ch >= app.bufs.len() {
            continue;
        }
        let data: Vec<(f64, f64)> = app.bufs[ch]
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as f64 / hz, v.clamp(-y_range, y_range)))
            .collect();

        draw_channel(frame, rows[ch], ch, &data, app);
    }
}

fn draw_channel(frame: &mut Frame, area: Rect, ch: usize, data: &[(f64, f64)], app: &App) {
    let color = COLORS[ch % COLORS.len()];
    let y_range = app.y_range();
    let name = app
        .channel_labels
        .get(ch)
        .map(|s| s.as_str())
        .unwrap_or("?");

    let (min_v, max_v, rms_v) = {
        let buf = &app.bufs[ch];
        if buf.is_empty() {
            (0.0_f64, 0.0_f64, 0.0_f64)
        } else {
            let min = buf.iter().copied().fold(f64::INFINITY, f64::min);
            let max = buf.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let rms = (buf.iter().map(|&v| v * v).sum::<f64>() / buf.len() as f64).sqrt();
            (min, max, rms)
        }
    };

    let clipping = max_v > y_range || min_v < -y_range;
    let border_color = if clipping { Color::Red } else { color };

    let clip_tag = if clipping { " [CLIP]" } else { "" };
    let smooth_tag = if app.smooth { " [SMOOTH]" } else { "" };
    let title = format!(
        " {name}  min:{}  max:{}  rms:{}{clip_tag}{smooth_tag} ",
        format_voltage(min_v),
        format_voltage(max_v),
        format_voltage(rms_v),
    );

    let y_labels: Vec<String> = [-1.0, -0.5, 0.0, 0.5, 1.0]
        .iter()
        .map(|&f| format_voltage(f * y_range))
        .collect();

    let x_labels = vec![
        "0s".to_string(),
        format!("{:.1}s", WINDOW_SECS / 2.0),
        format!("{:.0}s", WINDOW_SECS),
    ];

    let smoothed: Vec<(f64, f64)> = if app.smooth {
        smooth_signal(data, SMOOTH_WINDOW)
    } else {
        vec![]
    };

    let datasets: Vec<Dataset> = if app.smooth {
        vec![
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(DIM_COLORS[ch % DIM_COLORS.len()]))
                .data(data),
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(color))
                .data(&smoothed),
        ]
    } else {
        vec![Dataset::default()
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(color))
            .data(data)]
    };

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(Span::styled(
                    title,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        )
        .x_axis(
            Axis::default()
                .bounds([0.0, WINDOW_SECS])
                .labels(x_labels)
                .style(Style::default().fg(Color::DarkGray)),
        )
        .y_axis(
            Axis::default()
                .bounds([-y_range, y_range])
                .labels(y_labels)
                .style(Style::default().fg(Color::DarkGray)),
        );

    frame.render_widget(chart, area);
}

// ── Footer ───────────────────────────────────────────────────────────────────

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let pause_span = if app.paused {
        Span::styled(
            "  PAUSED",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };

    let keys = Line::from(vec![
        Span::raw(" "),
        key("[Tab]"),
        Span::raw("Devices  "),
        key("[d]"),
        Span::raw("Disconnect  "),
        key("[+/-]"),
        Span::raw("Scale  "),
        key("[a]"),
        Span::raw("Auto  "),
        key("[v]"),
        Span::raw(if app.smooth { "Raw  " } else { "Smooth  " }),
        key("[p]"),
        Span::raw("Pause  "),
        key("[r]"),
        Span::raw("Resume  "),
        key("[c]"),
        Span::raw("Clear  "),
        key("[q]"),
        Span::raw("Quit"),
        pause_span,
    ]);

    let second_line = match &app.mode {
        AppMode::NoDevices => {
            let detail = app
                .last_error
                .as_deref()
                .map(|e| format!(" Error: {e}"))
                .unwrap_or_default();
            Line::from(Span::styled(
                format!(
                    " No amplifiers found. Make sure the eego device is connected via USB.{detail} Retrying..."
                ),
                Style::default().fg(Color::Yellow),
            ))
        }
        _ => {
            let bat = app
                .battery_level
                .map(|b| format!("{}%", b))
                .unwrap_or_else(|| "N/A".into());
            let charging = app
                .is_charging
                .map(|c| if c { " (charging)" } else { "" })
                .unwrap_or("");
            Line::from(vec![
                Span::raw(" "),
                Span::styled("Battery ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{bat}{charging}"),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw("   "),
                Span::styled("Channels ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", app.num_channels),
                    Style::default().fg(Color::Magenta),
                ),
                Span::raw("   "),
                Span::styled("Rate ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} Hz", app.sample_rate as i32),
                    Style::default().fg(Color::Green),
                ),
            ])
        }
    };

    frame.render_widget(
        Paragraph::new(vec![keys, second_line]).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

// ── Device picker overlay ────────────────────────────────────────────────────

fn draw_device_picker(frame: &mut Frame, area: Rect, app: &App) {
    let n = app.picker_entries.len().max(1);
    let inner_h = n as u16 + 4;
    let box_h = inner_h + 2;
    let box_w = (area.width * 60 / 100).max(52).min(area.width);
    let x = area.x + (area.width.saturating_sub(box_w)) / 2;
    let y = area.y + (area.height.saturating_sub(box_h)) / 2;
    let popup = Rect::new(x, y, box_w, box_h.min(area.height));

    frame.render_widget(Clear, popup);

    let title = if app.picker_scanning {
        format!(
            " {} Scanning...  ({} found) ",
            spinner_str(),
            app.picker_entries.len()
        )
    } else {
        format!(" Select Amplifier  ({} found) ", app.picker_entries.len())
    };

    frame.render_widget(
        Block::default()
            .title(Span::styled(
                title,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White)),
        popup,
    );

    let inner = popup.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    let hint_h = 2u16;
    let [list_area, _, hint_area] = Layout::vertical([
        Constraint::Length(inner.height.saturating_sub(hint_h + 1)),
        Constraint::Length(1),
        Constraint::Length(hint_h),
    ])
    .areas(inner);

    let items: Vec<ListItem> = if app.picker_entries.is_empty() {
        vec![ListItem::new(Span::styled(
            "  No amplifiers found - check USB connection",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.picker_entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let connected = app.picker_connected_idx == Some(i);
                let (bullet, color, suffix) = if connected {
                    ("\u{25cf} ", Color::Green, "  <- connected")
                } else {
                    ("  ", Color::White, "")
                };
                ListItem::new(Span::styled(
                    format!("{bullet}{entry}{suffix}"),
                    Style::default().fg(color),
                ))
            })
            .collect()
    };

    let mut list_state = ListState::default();
    if !app.picker_entries.is_empty() {
        list_state.select(Some(app.picker_cursor));
    }

    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("\u{25b6} "),
        list_area,
        &mut list_state,
    );

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                key(" [Up/Down]"),
                Span::raw(" Navigate  "),
                key("[Enter]"),
                Span::raw(" Connect  "),
                key("[Esc]"),
                Span::raw(" Close"),
            ]),
            Line::from(Span::styled(
                " Amplifier list is refreshed after every scan",
                Style::default().fg(Color::DarkGray),
            )),
        ]),
        hint_area,
    );
}
