//! Active 1-to-1 call view rendering for TUI client.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, Paragraph},
};

use crate::tui::app::TuiApp;

/// Render active 1-to-1 call view.
pub fn render_call_view(f: &mut Frame, app: &TuiApp, target: &str, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Call status header
            Constraint::Length(3), // Mic Level
            Constraint::Length(3), // Speaker Level
            Constraint::Min(2),    // Call details
        ])
        .split(area);

    let call_header = vec![
        Line::from(vec![
            Span::styled(
                " Connected Target: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                target,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw(" Connection State: "),
            Span::styled(
                "ENCRYPTED P2P CALL",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let header_block = Block::default()
        .title(" Active 1-to-1 Voice Call ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(Paragraph::new(call_header).block(header_block), chunks[0]);

    // Mic Level
    let (mic_title, mic_color) = if app.is_muted {
        (" Mic Level (MUTED - Press [m] to unmute) ", Color::Red)
    } else {
        (" Mic Level ", Color::Green)
    };
    let mic_gauge = Gauge::default()
        .block(Block::default().title(mic_title).borders(Borders::ALL))
        .gauge_style(Style::default().fg(mic_color))
        .ratio(app.input_level.clamp(0.0, 1.0) as f64);
    f.render_widget(mic_gauge, chunks[1]);

    // Speaker Level
    let spk_gauge = Gauge::default()
        .block(
            Block::default()
                .title(" Speaker Level ")
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Cyan))
        .ratio(app.output_level.clamp(0.0, 1.0) as f64);
    f.render_widget(spk_gauge, chunks[2]);

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
        Line::from("Press [h] to hang up call."),
    ];
    let details_block = Block::default()
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
    fn test_render_call_view() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let app = TuiApp::new(wpapi::Configuration::default(), tx);

        terminal
            .draw(|f| {
                let area = f.area();
                render_call_view(f, &app, "test_target_user", area);
            })
            .unwrap();
    }
}
