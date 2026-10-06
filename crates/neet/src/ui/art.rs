//! The Home art: a sleeping cat behind the neet wordmark, under a starry sky.
//!
//! The cat and wordmark are drawn from `ART`, which can be edited here
//! directly. Braille characters and `Z` / `z` are the cat, and block and box
//! drawing characters are the wordmark. The stars are not part of `ART`. They
//! are scattered evenly across the whole screen, corners included, and the
//! menu's boxes are drawn on top.

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

/// The share of cells holding a star, before close neighbours are thinned.
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

/// Small dots, in Tokyo Night's comment blue, light enough to see on a
/// dark background.
const FAINT: Color = Color::Rgb(0x56, 0x5f, 0x89);

/// Bright stars, in Tokyo Night's text color.
const BRIGHT: Color = Color::Rgb(0xa9, 0xb1, 0xd6);

fn star_style(c: char) -> Style {
    match c {
        '·' => Style::new().fg(FAINT),
        _ => Style::new().fg(BRIGHT),
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

/// The star at `(x, y)`, before spacing is applied
fn raw(x: i32, y: i32) -> Option<char> {
    let h = hash(x, y);
    if unit(h) >= STAR_DENSITY {
        return None;
    }
    Some(match (h >> 16) & 0xFF {
        0..170 => '·',
        170..215 => '*',
        215..240 => '✦',
        _ => '+',
    })
}

/// The star at `(x, y)`, skipped when a close neighbour already has one.
fn star(x: i32, y: i32) -> Option<char> {
    let neighbours = [(-1, 0), (-2, 0), (0, -1)];
    let crowded = neighbours
        .iter()
        .any(|&(ox, oy)| raw(x + ox, y + oy).is_some());
    if crowded { None } else { raw(x, y) }
}

/// The Home art: stars across the whole area it is drawn in, with the cat
/// and wordmark centred in `art`.
pub struct Night {
    pub art: Rect,
}

impl Widget for Night {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (width, height) = size();
        let left = self.art.x + self.art.width.saturating_sub(width) / 2;
        let top = self.art.y + self.art.height.saturating_sub(height) / 2;

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

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if near_art(x, y) {
                    continue;
                }
                if let Some(c) = star(i32::from(x), i32::from(y))
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
    use super::{BRIGHT, FAINT, MOONLIGHT, Night, size};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;

    const STARS: [&str; 4] = ["·", "*", "✦", "+"];

    fn render(width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        Night { art: area }.render(area, &mut buf);
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
    fn colours_the_art_and_stars_in_the_night_palette() {
        let buf = render(60, 23);
        let fg_of = |symbol: &str| {
            buf.content()
                .iter()
                .find(|cell| cell.symbol() == symbol)
                .map(|cell| cell.fg)
        };

        assert_eq!(fg_of("█"), Some(MOONLIGHT));
        assert_eq!(fg_of("⣿"), Some(MOONLIGHT));
        assert_eq!(fg_of("·"), Some(FAINT));
        assert_eq!(fg_of("*"), Some(BRIGHT));
    }

    #[test]
    fn fills_every_corner_with_stars() {
        let buf = render(90, 60);
        let has_star = |xs: std::ops::Range<u16>, ys: std::ops::Range<u16>| {
            ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
                .any(|(x, y)| STARS.contains(&buf[(x, y)].symbol()))
        };
        for (xs, ys) in [
            (0..10, 0..6),
            (80..90, 0..6),
            (0..10, 54..60),
            (80..90, 54..60),
        ] {
            assert!(
                has_star(xs.clone(), ys.clone()),
                "no stars at {xs:?}, {ys:?}"
            );
        }
    }

    #[test]
    fn stars_reach_the_far_rows_of_a_tall_screen() {
        let buf = render(90, 60);
        let has_star = |rows: std::ops::Range<u16>| {
            rows.flat_map(|y| (0..90).map(move |x| (x, y)))
                .any(|(x, y)| STARS.contains(&buf[(x, y)].symbol()))
        };
        assert!(has_star(2..8), "no stars near the top");
        assert!(has_star(52..58), "no stars near the bottom");
    }

    #[test]
    fn stars_stay_in_the_same_place() {
        assert_eq!(render(60, 23), render(60, 23));
    }
}
