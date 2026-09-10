//! Standby and idle state view rendering for TUI client.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::tui::app::TuiApp;

/// Render standby / idle status panel in TUI layout.
pub fn render_standby_view(f: &mut Frame, app: &TuiApp, area: Rect) {
    let standby_info = vec![
        Line::from(vec![
            Span::styled(
                " Status: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Ready for incoming or outgoing calls",
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                " Assigned Address: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(
                app.my_address
                    .as_ref()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|| "Assigning...".into()),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press [c] to initiate 1-to-1 call, [r] to join SFU group room, [l] to list online peers.",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let block = Block::default()
        .title(" Standby / Idle Overview ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Green));

    f.render_widget(Paragraph::new(standby_info).block(block), area);
}
