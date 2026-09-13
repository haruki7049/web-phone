//! SFU group room view rendering for TUI client.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::tui::app::TuiApp;

/// Render SFU group room view.
pub fn render_room_view(f: &mut Frame, app: &TuiApp, room: &str, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Room status header
            Constraint::Length(3), // Mic Level
            Constraint::Length(3), // Speaker Level
            Constraint::Min(2),    // Room details / active speakers
        ])
        .split(area);

    let room_header = vec![
        Line::from(vec![
            Span::styled(
                " SFU Room Address: ",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                room,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw(" Mode: "),
            Span::styled(
                "SFU MULTI-PARTY GROUP CALL",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let header_block = Block::default()
        .title(" SFU Group Voice Room ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Magenta));
    f.render_widget(Paragraph::new(room_header).block(header_block), chunks[0]);

    // Render shared audio gauges (Mic and Speaker)
    super::render_audio_gauges(
        f,
        app,
        chunks[1],
        chunks[2],
        " Room Audio Level ",
        Color::Magenta,
    );

    let details_text = vec![
        Line::from(format!(
            "Mute: {}",
            if app.is_muted {
                "MUTED (Press [m] to unmute)"
            } else {
                "ACTIVE (Press [m] to mute)"
            }
        )),
        Line::from(format!(
            "Echo Back: {} (Press [e] to toggle)",
            if app.config.audio.allow_echoback {
                "ENABLED"
            } else {
                "DISABLED"
            }
        )),
        Line::from(format!(
            "Auto Accept: {} (Press [a] to toggle)",
            if app.config.client.auto_accept {
                "ENABLED"
            } else {
                "DISABLED"
            }
        )),
        Line::from("Press [h] to leave room."),
    ];
    let details_block = Block::default()
        .title(" Room Info & Active Speakers ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(details_text).block(details_block), chunks[3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_room_view() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let app = TuiApp::new(wpapi::Configuration::default(), tx);

        terminal
            .draw(|f| {
                let area = f.area();
                render_room_view(f, &app, "test_room_address", area);
            })
            .unwrap();
    }
}
