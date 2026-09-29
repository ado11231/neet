//! The Home art: a sleeping cat behind the neet wordmark, under a starry sky.
//!
//! The cat and wordmark are drawn from `ART`, which can be edited here
//! directly. Braille characters and `Z` / `z` are the cat, and block and box
//! drawing characters are the wordmark. The stars are not part of `ART`. They
//! are scattered across the whole art column, thinning out towards uneven
//! edges, so the sky has no hard border.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

// Starts with a newline so the first row keeps its leading spaces.
pub const ART: &str = "
           ⣦⡀              ⣠⡆       Z
          ⢸⣿⠻⣦⣀         ⢀⣠⡾⢻⣿     z
          ⣾⡏ ⠈⠻⣷⣶⣿⣿⣿⣿⣿⣷⣶⡿⠋ ⠈⣿⡆  z
         ⢰⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷
         ⣼⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡄
         ⣿⣿⣿⠻⣿⣿⣿⡿⢻⣿⣿⣿⣿⠻⣿⣿⣿⡿⢻⣿⣿⡇
         ⢻⣿⣿⣦⣌⣉⣉⣤⣾⣿⣿⣿⣿⣦⣌⣉⣉⣤⣾⣿⣿⠃
          ⠻⣿⣿⣿⣿⣿⣿⣿⣷⡄⣴⣿⣿⣿⣿⣿⣿⣿⡿⠃
           ⠈⠛⠿⣿⣿⣿⣷⣭⣼⣬⣵⣿⣿⣿⡿⠟⠋
             ⣠⣶⣭⣭⣛⣛⣛⣛⣛⣫⣭⣵⣦⡀   ⢀⣴⣄⡀
           ⢠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣦   ⠈⢻⣇
          ⢠⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣧   ⣼⡟
   ███╗   ██╗███████╗███████╗████████╗
   ████╗  ██║██╔════╝██╔════╝╚══██╔══╝
   ██╔██╗ ██║█████╗  █████╗     ██║
   ██║╚██╗██║██╔══╝  ██╔══╝     ██║
   ██║ ╚████║███████╗███████╗   ██║
   ╚═╝  ╚═══╝╚══════╝╚══════╝   ╚═╝
";

/// The blue shared by the cat and the wordmark: Tokyo Night moon's `#82aaff`,
/// the blue of the dashboard logo in `LazyVim`.
const MOONLIGHT: Color = Color::Rgb(0x82, 0xaa, 0xff);

/// The share of cells holding a star in the middle of the sky.
const STAR_DENSITY: f64 = 0.26;

/// The rows of `ART`, without the newline it starts with.
fn rows() -> std::str::Lines<'static> {
    ART.strip_prefix('\n').unwrap_or(ART).lines()
}

fn art_style(c: char) -> Style {
    match c {
        'Z' | 'z' => Style::new().fg(MOONLIGHT).add_modifier(Modifier::DIM),
        _ => Style::new().fg(MOONLIGHT),
    }
}

fn star_style(c: char) -> Style {
    match c {
        '·' => Style::new().fg(Color::DarkGray),
        _ => Style::new().fg(Color::Gray),
    }
}

/// How many columns and rows the cat and wordmark take up.
pub fn size() -> (u16, u16) {
    let width = rows().map(|row| row.chars().count()).max().unwrap_or(0);
    let height = rows().count();
    (
        u16::try_from(width).unwrap_or(u16::MAX),
        u16::try_from(height).unwrap_or(u16::MAX),
    )
}

/// Mixes a position into a stable pseudo random number, so the stars stay
/// put between frames.
fn hash(x: i32, y: i32) -> u64 {
    let mut z = (u64::from(x.cast_unsigned()) << 32 | u64::from(y.cast_unsigned()))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A number from 0 to 1 taken from 16 bits of `bits`.
fn unit(bits: u64) -> f64 {
    f64::from(u16::try_from(bits & 0xFFFF).unwrap_or(0)) / f64::from(u16::MAX)
}

/// The sky, as an oval around the art whose edge is roughened with noise.
struct Sky {
    radius_x: f64,
    radius_y: f64,
}

impl Sky {
    /// The star at `(dx, dy)` from the centre, before spacing is applied.
    fn raw(&self, dx: i32, dy: i32) -> Option<char> {
        let h = hash(dx, dy);
        let (nx, ny) = (f64::from(dx) / self.radius_x, f64::from(dy) / self.radius_y);
        let distance = (nx * nx + ny * ny).sqrt() + unit(h >> 32) * 0.5 - 0.25;
        // Full density inside 0.6 of the radius, fading to none at 1.1.
        let fade = ((1.1 - distance) / 0.5).clamp(0.0, 1.0);
        if unit(h) >= STAR_DENSITY * fade {
            return None;
        }
        Some(match (h >> 16) & 0xFF {
            0..170 => '·',
            170..215 => '*',
            215..240 => '✦',
            _ => '+',
        })
    }

    /// The star at `(dx, dy)`, skipped when a close neighbour already has one.
    fn star(&self, dx: i32, dy: i32) -> Option<char> {
        let neighbours = [(-1, 0), (-2, 0), (0, -1)];
        let crowded = neighbours
            .iter()
            .any(|&(ox, oy)| self.raw(dx + ox, dy + oy).is_some());
        if crowded { None } else { self.raw(dx, dy) }
    }
}

/// The Home art: stars across `area`, with the cat and wordmark centred on top.
pub struct Night;

impl Widget for Night {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (width, height) = size();
        let left = area.x + area.width.saturating_sub(width) / 2;
        let top = area.y + area.height.saturating_sub(height) / 2;

        let mut art = Vec::new();
        for (row, line) in (0_u16..).zip(rows()) {
            for (col, c) in (0_u16..).zip(line.chars()) {
                if c != ' ' {
                    art.push((left.saturating_add(col), top.saturating_add(row), c));
                }
            }
        }

        // Keep stars a row above and below, and two columns either side, of the art.
        let near_art = |x: u16, y: u16| {
            art.iter()
                .any(|&(ax, ay, _)| x.abs_diff(ax) <= 2 && y.abs_diff(ay) <= 1)
        };

        let sky = Sky {
            radius_x: (f64::from(width) / 2.0 + 16.0).min(f64::from(area.width) / 2.0 + 3.0),
            radius_y: (f64::from(height) / 2.0 + 5.0).min(f64::from(area.height) / 2.0 + 2.0),
        };
        let centre_x = i32::from(left) + i32::from(width / 2);
        let centre_y = i32::from(top) + i32::from(height / 2);
        for y in area.top()..area.bottom() {
            // The last column stays clear, so no star touches the menu border.
            for x in area.left()..area.right().saturating_sub(1) {
                if near_art(x, y) {
                    continue;
                }
                if let Some(c) = sky.star(i32::from(x) - centre_x, i32::from(y) - centre_y)
                    && let Some(cell) = buf.cell_mut((x, y))
                {
                    cell.set_char(c).set_style(star_style(c));
                }
            }
        }

        for (x, y, c) in art {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(c).set_style(art_style(c));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MOONLIGHT, Night, size};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::widgets::Widget;

    const STARS: [&str; 4] = ["·", "*", "✦", "+"];

    fn render(width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        Night.render(area, &mut buf);
        buf
    }

    #[test]
    fn fits_the_home_screen() {
        let (width, height) = size();
        // The art column is 45% of a 90 column terminal, and a 24 row terminal
        // leaves 23 rows above the footer.
        assert!(width <= 40, "art is {width} columns wide");
        assert!(height <= 23, "art is {height} rows tall");
    }

    #[test]
    fn colours_the_art_blue_and_the_stars_grey() {
        let buf = render(60, 23);
        let fg_of = |symbol: &str| {
            buf.content()
                .iter()
                .find(|cell| cell.symbol() == symbol)
                .map(|cell| cell.fg)
        };

        assert_eq!(fg_of("█"), Some(MOONLIGHT));
        assert_eq!(fg_of("⣿"), Some(MOONLIGHT));
        assert_eq!(fg_of("·"), Some(Color::DarkGray));
        assert_eq!(fg_of("*"), Some(Color::Gray));
    }

    #[test]
    fn scatters_stars_but_leaves_the_corners_dark() {
        let buf = render(60, 23);
        let is_star = |x: u16, y: u16| STARS.contains(&buf[(x, y)].symbol());
        let stars = (0..23)
            .flat_map(|y| (0..60).map(move |x| (x, y)))
            .filter(|&(x, y)| is_star(x, y))
            .count();

        assert!(stars >= 30, "only {stars} stars");
        for (x, y) in [(0, 0), (1, 0), (0, 1), (59, 0), (58, 0), (0, 22), (59, 22)] {
            assert!(!is_star(x, y), "star in the corner at {x}, {y}");
        }
    }

    #[test]
    fn stars_stay_in_the_same_place() {
        assert_eq!(render(60, 23), render(60, 23));
    }
}
