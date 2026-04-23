use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use super::app::AppState;

pub fn render(frame: &mut Frame<'_>, app: &mut AppState) {
    let header_lines = wrap_text_block(app.header_lines(), frame.area().width);
    let footer_lines = wrap_text_block([app.controls_hint()], frame.area().width);
    let chunks = Layout::vertical([
        Constraint::Length(line_count(&header_lines)),
        Constraint::Min(4),
        Constraint::Length(line_count(&footer_lines)),
    ])
    .split(frame.area());

    frame.render_widget(
        Paragraph::new(header_lines).style(Style::default().fg(Color::White)),
        chunks[0],
    );

    if app.show_help {
        render_help(frame, chunks[1]);
    } else {
        frame.render_stateful_widget(app.image_widget(), chunks[1], &mut app.image);
    }

    frame.render_widget(
        Paragraph::new(footer_lines).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

fn line_count(lines: &[Line<'static>]) -> u16 {
    lines.len().max(1).min(u16::MAX as usize) as u16
}

fn wrap_text_block<I, S>(blocks: I, width: u16) -> Vec<Line<'static>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let width = width.max(1) as usize;
    let mut lines = Vec::new();

    for block in blocks {
        let wrapped = wrap_text_line(block.as_ref(), width);
        if wrapped.is_empty() {
            lines.push(Line::raw(String::new()));
            continue;
        }

        lines.extend(wrapped.into_iter().map(Line::raw));
    }

    if lines.is_empty() {
        lines.push(Line::raw(String::new()));
    }

    lines
}

fn wrap_text_line(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let word_len = word.chars().count();

        if word_len > width {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            lines.extend(split_long_word(word, width));
            continue;
        }

        let current_len = current.chars().count();
        let needed = if current.is_empty() {
            word_len
        } else {
            current_len + 1 + word_len
        };

        if needed <= width {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }

    lines
}

fn split_long_word(word: &str, width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for ch in word.chars() {
        current.push(ch);
        if current.chars().count() == width {
            chunks.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
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
            Line::from("z: cycle display size"),
            Line::from("b: cycle playback render mode"),
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

#[cfg(test)]
mod tests {
    use super::{split_long_word, wrap_text_line};

    #[test]
    fn wrap_text_line_wraps_on_word_boundaries_when_possible() {
        assert_eq!(
            wrap_text_line("axis=axial slice=80 cmap=gray", 16),
            vec![
                "axis=axial".to_string(),
                "slice=80".to_string(),
                "cmap=gray".to_string()
            ]
        );
    }

    #[test]
    fn wrap_text_line_splits_overlong_tokens() {
        assert_eq!(
            split_long_word("sub-102041_ses-1_rec-refaced_T1w.nii.gz", 12),
            vec![
                "sub-102041_s".to_string(),
                "es-1_rec-ref".to_string(),
                "aced_T1w.nii".to_string(),
                ".gz".to_string()
            ]
        );
    }
}
