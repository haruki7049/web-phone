//! Modular TUI view rendering components.

pub mod call;
pub mod room;
pub mod standby;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Gauge};

use super::app::TuiApp;

/// Render shared microphone and speaker audio level gauges for call and room views.
pub fn render_audio_gauges(
    f: &mut Frame,
    app: &TuiApp,
    mic_area: Rect,
    spk_area: Rect,
    spk_title: &str,
    spk_color: Color,
) {
    let (mic_title, mic_color) = if app.is_muted {
        (" Mic Level (MUTED - Press [m] to unmute) ", Color::Red)
    } else {
        (" Mic Level ", Color::Green)
    };

    let mic_gauge = Gauge::default()
        .block(Block::default().title(mic_title).borders(Borders::ALL))
        .gauge_style(Style::default().fg(mic_color))
        .ratio(app.input_level.clamp(0.0, 1.0) as f64);
    f.render_widget(mic_gauge, mic_area);

    let spk_gauge = Gauge::default()
        .block(Block::default().title(spk_title).borders(Borders::ALL))
        .gauge_style(Style::default().fg(spk_color))
        .ratio(app.output_level.clamp(0.0, 1.0) as f64);
    f.render_widget(spk_gauge, spk_area);
}
