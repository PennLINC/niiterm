use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use ratatui_image::Resize;

use super::app::AppState;

pub fn render(frame: &mut Frame<'_>, app: &mut AppState) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .split(frame.area());

    frame.render_widget(
        Paragraph::new(app.status_line()).style(Style::default().fg(Color::White)),
        chunks[0],
    );

    if app.show_help {
        render_help(frame, chunks[1]);
    } else {
        let image = app.image_widget().resize(Resize::Fit(None));
        frame.render_stateful_widget(image, chunks[1], &mut app.image);
    }

    frame.render_widget(
        Paragraph::new(app.controls_hint()).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

fn render_help(frame: &mut Frame<'_>, area: Rect) {
    let popup = centered_rect(area, 80, 70);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "niiterm Help",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Left/Right or h/l: previous/next slice"),
            Line::from("Up/Down or j/k: move slice by 10"),
            Line::from("H/L: previous/next volume"),
            Line::from("a: cycle axis"),
            Line::from("space: play/pause 4D series"),
            Line::from("+: increase FPS"),
            Line::from("-: decrease FPS"),
            Line::from("c: cycle colormap"),
            Line::from("w: cycle window preset"),
            Line::from("g: jump to middle slice"),
            Line::from("?: close help"),
            Line::from("q or esc: quit"),
        ])
        .wrap(Wrap { trim: true })
        .block(Block::default().title("Help").borders(Borders::ALL)),
        popup,
    );
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - height_percent) / 2),
        Constraint::Percentage(height_percent),
        Constraint::Percentage((100 - height_percent) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - width_percent) / 2),
        Constraint::Percentage(width_percent),
        Constraint::Percentage((100 - width_percent) / 2),
    ])
    .split(vertical[1])[1]
}
