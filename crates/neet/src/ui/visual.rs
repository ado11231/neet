//! Shared presentation rules; the Home artwork uses its own styles.

use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Wrap};

/// Match the existing Home artwork without changing its renderer.
pub const ACCENT: Color = Color::Rgb(0x82, 0xaa, 0xff);
pub const HEADING: Style = Style::new().fg(ACCENT).add_modifier(Modifier::BOLD);
pub const SELECTED: Style = Style::new().add_modifier(Modifier::BOLD);

pub fn block<'a>() -> Block<'a> {
    Block::bordered().title_style(HEADING)
}

/// Use the same wrapping engine to measure and render paragraphs.
pub fn wrapped_rows(lines: &[Line<'_>], width: u16) -> u16 {
    u16::try_from(
        Paragraph::new(lines.to_vec())
            .wrap(Wrap { trim: false })
            .line_count(width.max(1)),
    )
    .unwrap_or(u16::MAX)
}

/// `sections` as a stack of boxes filling `area`, one a section. The first
/// is titled `title`. A section that starts with a heading line, styled
/// [`HEADING`], takes that heading as its box's title instead, and so does
/// the first when `title` is blank. The room left over is shared evenly, so
/// a tall area has no empty space below the boxes. Sections that do not fit
/// are left out, the last first. Leading spaces are kept, so right aligned
/// values stay lined up.
pub fn sections(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    title: &str,
    mut sections: Vec<Vec<Line<'static>>>,
) {
    let first = if title.trim().is_empty() {
        sections.first_mut().and_then(heading).unwrap_or_default()
    } else {
        title.trim().to_string()
    };
    let block = block()
        .title(format!(" {first} "))
        .padding(ratatui::widgets::Padding::horizontal(1));
    sections_in(frame, area, block, sections);
}

/// The first line of `lines` as a title, taken out, when it is a heading
fn heading(lines: &mut Vec<Line<'static>>) -> Option<String> {
    let first = lines.first()?;
    if first.style != HEADING {
        return None;
    }
    let text = first.to_string();
    lines.remove(0);
    Some(text)
}

/// [`sections`] with the first box given, such as one with a bottom title
pub fn sections_in(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    first: Block<'static>,
    sections: Vec<Vec<Line<'static>>>,
) {
    use ratatui::layout::Rect;
    use ratatui::widgets::Padding;
    let mut boxes: Vec<(Block<'static>, Vec<Line<'static>>)> = Vec::new();
    let mut first = Some(first);
    for mut lines in sections {
        let block = if let Some(first) = first.take() {
            first
        } else {
            let title = heading(&mut lines).unwrap_or_default();
            let block = block().padding(Padding::horizontal(1));
            if title.is_empty() {
                block
            } else {
                block.title(format!(" {title} "))
            }
        };
        boxes.push((block, lines));
    }
    // Each box's text, and its border
    let inner = area.width.saturating_sub(4);
    let mut heights: Vec<u16> = boxes
        .iter()
        .map(|(_, lines)| wrapped_rows(lines, inner).max(1) + 2)
        .collect();
    while heights.len() > 1 && heights.iter().sum::<u16>() > area.height {
        heights.pop();
        boxes.pop();
    }
    let shares = u16::try_from(heights.len()).unwrap_or(1).max(1);
    let leftover = area.height.saturating_sub(heights.iter().sum());
    let mut y = area.y;
    for (index, ((block, lines), height)) in boxes.into_iter().zip(heights).enumerate() {
        // The last box takes what is left after the even shares.
        let share = if usize::from(shares) == index + 1 {
            area.bottom().saturating_sub(y)
        } else {
            (height + leftover / shares).min(area.bottom().saturating_sub(y))
        };
        let rect = Rect::new(area.x, y, area.width, share);
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(block),
            rect,
        );
        y += share;
    }
}

/// Keys remain distinct from their actions, including when the footer wraps.
pub fn hints(text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, hint) in text.split(" · ").enumerate() {
        if index > 0 {
            spans.push(Span::raw(" · "));
        }
        let (key, action) = hint.split_once(' ').unwrap_or((hint, ""));
        spans.push(Span::styled(key.to_string(), HEADING));
        spans.push(Span::raw(format!(" {action}")));
    }
    Line::from(spans)
}

/// Shared column geometry for headers and rows. Widths include neither the
/// border/padding nor the selection gutter. Optional columns disappear in
/// order before flexible name/path columns lose their minimum width.
pub struct Columns<const N: usize> {
    sizes: [u16; N],
    visible: [bool; N],
}

impl<const N: usize> Columns<N> {
    pub fn new(width: u16, specs: [(u16, u16); N], optional: &[usize], selected: bool) -> Self {
        let room = width.saturating_sub(4 + if selected { 2 } else { 0 });
        let mut visible = [true; N];
        let needed = |visible: &[bool; N]| -> u16 {
            let count = visible.iter().filter(|&&show| show).count();
            specs
                .iter()
                .zip(visible)
                .filter(|(_, show)| **show)
                .map(|(&(min, _), _)| min)
                .sum::<u16>()
                .saturating_add(u16::try_from(count.saturating_sub(1) * 2).unwrap_or(u16::MAX))
        };
        for &index in optional {
            if needed(&visible) <= room {
                break;
            }
            visible[index] = false;
        }
        let mut extra = room.saturating_sub(needed(&visible));
        let mut weight: u16 = specs
            .iter()
            .zip(visible)
            .filter(|(_, show)| *show)
            .map(|(&(_, weight), _)| weight)
            .sum();
        let mut sizes = std::array::from_fn(|index| {
            if !visible[index] {
                return 0;
            }
            let (min, share) = specs[index];
            let add = if weight == 0 {
                0
            } else {
                u16::try_from(u32::from(extra) * u32::from(share) / u32::from(weight))
                    .unwrap_or(extra)
            };
            extra -= add;
            weight -= share;
            min + add
        });
        let mut deficit = needed(&visible).saturating_sub(room);
        for (index, &(_, weight)) in specs.iter().enumerate() {
            if visible[index] && weight > 0 {
                let reduce = deficit.min(sizes[index].saturating_sub(4));
                sizes[index] -= reduce;
                deficit -= reduce;
            }
        }
        Self { sizes, visible }
    }

    pub fn width(&self, index: usize) -> usize {
        usize::from(self.sizes[index])
    }

    pub fn widths(&self) -> Vec<Constraint> {
        self.sizes
            .iter()
            .zip(self.visible)
            .filter(|(_, show)| *show)
            .map(|(&width, _)| Constraint::Length(width))
            .collect()
    }

    pub fn row<'a>(&self, cells: [Cell<'a>; N]) -> Row<'a> {
        Row::new(
            cells
                .into_iter()
                .zip(self.visible)
                .filter(|(_, show)| *show)
                .map(|(cell, _)| cell),
        )
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::ui::app::{Context, Screen};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Layout;

    pub const SIZES: [(u16, u16); 4] = [(60, 20), (80, 24), (100, 30), (140, 40)];

    /// Render the real screen with the same footer geometry as the app.
    pub fn render(
        name: &str,
        screen: &mut dyn Screen,
        context: &Context,
        width: u16,
        height: u16,
    ) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                let hints = hints(screen.hints());
                let footer_height = wrapped_rows(std::slice::from_ref(&hints), width);
                let [body, footer] =
                    Layout::vertical([Constraint::Fill(1), Constraint::Length(footer_height)])
                        .areas(frame.area());
                screen.draw(frame, body, context);
                frame.render_widget(Paragraph::new(hints).wrap(Wrap { trim: false }), footer);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        if name != "home" {
            for cell in buffer.content() {
                assert!(!matches!(cell.fg, Color::Gray | Color::DarkGray));
                assert!(!cell.modifier.contains(Modifier::DIM));
            }
        }
        // Opt-in text previews make every tested size inspectable without running cleanup.
        if std::env::var_os("NEET_TUI_PREVIEW").is_some() {
            println!("\n{name} {width}x{height}\n{}", text(&buffer));
        }
        buffer
    }

    pub fn text(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn position(buffer: &Buffer, value: &str) -> (u16, u16) {
        let mut found = Vec::new();
        for y in 0..buffer.area.height {
            let line: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            if let Some(offset) = line.find(value) {
                found.push((
                    u16::try_from(crate::ui::format::display_width(&line[..offset])).unwrap(),
                    y,
                ));
            }
        }
        found
            .into_iter()
            .min()
            .unwrap_or_else(|| panic!("{value:?} missing:\n{}", text(buffer)))
    }

    pub fn aligned(buffer: &Buffer, header: &str, value: &str) {
        let (header_end, header_y) = (0..buffer.area.height)
            .find_map(|y| {
                let line: String = (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect();
                let offset = line.find(header)?;
                ["Name", "Rule", "Item", "Runtime", "Kind"]
                    .iter()
                    .any(|label| line.contains(label))
                    .then(|| {
                        (
                            crate::ui::format::display_width(&line[..offset]) + header.len(),
                            y,
                        )
                    })
            })
            .expect("table header");
        assert_eq!(
            buffer[(u16::try_from(header_end - 1).unwrap(), header_y)].fg,
            ACCENT
        );
        let aligned = (header_y + 1..buffer.area.height).any(|y| {
            let line: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            line.match_indices(value).any(|(offset, _)| {
                crate::ui::format::display_width(&line[..offset]) + value.len() == header_end
            })
        });
        assert!(
            aligned,
            "{header} and {value} must share a right edge\n{}",
            text(buffer)
        );
    }
}
