use std::time::Duration;

use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;

/// Sizes from here are red, and from a fifth of it yellow.
pub const HUGE: u64 = 5_000_000_000;

/// `text`, red and bold when `bytes` is 5 GB or more, yellow from 1 GB, so
/// big items stand out.
pub fn size_span(bytes: u64, text: String) -> Span<'static> {
    let span = Span::raw(text);
    if bytes >= HUGE {
        span.red().bold()
    } else if bytes >= HUGE / 5 {
        span.yellow()
    } else {
        span
    }
}

/// Formats bytes the way Finder does, in powers of 1000.
#[allow(clippy::cast_precision_loss)] // One decimal place is all that is shown.
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 999.95 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Formats a count with commas, such as `1,394,799`.
pub fn count(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A bar `width` characters wide, filled to show `part` as a share of `total`.
pub fn bar(part: u64, total: u64, width: usize) -> String {
    let filled = if total == 0 {
        0
    } else {
        let width = u128::try_from(width).unwrap_or(0);
        let filled = (u128::from(part) * width + u128::from(total) / 2) / u128::from(total);
        usize::try_from(filled.min(width)).unwrap_or(0)
    };
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// A bar of `size` against `largest`, colored like the size: red from 5 GB,
/// yellow from 1 GB, and green below.
pub fn size_bar(size: u64, largest: u64, width: usize) -> Span<'static> {
    let bar = Span::raw(bar(size, largest, width));
    if size >= HUGE {
        bar.red()
    } else if size >= HUGE / 5 {
        bar.yellow()
    } else {
        bar.green()
    }
}

/// How long ago something was, in the largest whole unit, such as `3 days`.
pub fn age(elapsed: Duration) -> String {
    age_parts(elapsed).map_or_else(
        || "today".to_string(),
        |(value, unit)| format!("{value} {unit} ago"),
    )
}

/// Like [`age`], with the number right aligned in two places, so in a column
/// the ones line up and every unit starts at the same place: ` 3 days ago`,
/// `39 days ago`, `   today`.
pub fn age_aligned(elapsed: Duration) -> String {
    age_parts(elapsed).map_or_else(
        || format!("{:>2} today", ""),
        |(value, unit)| format!("{value:>2} {unit} ago"),
    )
}

/// The number and unit of an age, or `None` for today
fn age_parts(elapsed: Duration) -> Option<(u64, &'static str)> {
    const DAY: u64 = 24 * 60 * 60;
    let days = elapsed.as_secs() / DAY;
    let (value, unit, units) = match days {
        0 => return None,
        1..=59 => (days, "day", "days"),
        60..=364 => (days / 30, "month", "months"),
        _ => (days / 365, "year", "years"),
    };
    Some((value, if value == 1 { unit } else { units }))
}

/// `part` as a whole percent of `total`, rounded down.
pub fn percent(part: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    u64::try_from(u128::from(part) * 100 / u128::from(total)).unwrap_or(100)
}

/// Shortens `text` to `width` terminal cells by cutting out its middle, so a
/// file name keeps its start and its extension.
pub fn shorten_middle(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let keep = width - 1;
    let tail = suffix(text, keep / 2);
    let mut head = String::new();
    let room = keep - display_width(&tail);
    for part in text.graphemes(true) {
        if display_width(&head) + display_width(part) > room {
            break;
        }
        head.push_str(part);
    }
    format!("{head}…{tail}")
}

/// Terminal cells occupied by text, including wide and combining characters.
pub fn display_width(text: &str) -> usize {
    Line::from(text).width()
}

fn suffix(text: &str, width: usize) -> String {
    let mut used = 0;
    let parts: Vec<&str> = text
        .graphemes(true)
        .rev()
        .take_while(|part| {
            used += display_width(part);
            used <= width
        })
        .collect();
    parts.into_iter().rev().collect()
}

/// Shortens a folder path without splitting a grapheme or exceeding its cells.
pub fn shorten_path(path: &str, width: usize) -> String {
    if display_width(path) <= width {
        return path.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let head: String = path.split_inclusive('/').take(2).collect();
    let head = if display_width(&head) + 13 <= width {
        head
    } else {
        String::new()
    };
    let tail = suffix(path, width - display_width(&head) - 1);
    let tail = match tail.find('/') {
        Some(slash) if slash + 1 < tail.len() => &tail[slash..],
        _ => &tail,
    };
    format!("{head}…{tail}")
}

/// Room for a header and the widest formatted value beneath it.
pub fn column_width(header: &str, values: impl IntoIterator<Item = String>) -> u16 {
    u16::try_from(
        values
            .into_iter()
            .map(|value| display_width(&value))
            .chain([display_width(header)])
            .max()
            .unwrap_or(0),
    )
    .unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::{age, age_aligned, bar, count, percent, shorten_middle, shorten_path, size};
    use std::time::Duration;

    #[test]
    fn sizes_use_powers_of_1000() {
        assert_eq!(size(0), "0 B");
        assert_eq!(size(999), "999 B");
        assert_eq!(size(1000), "1.0 KB");
        assert_eq!(size(1_500_000), "1.5 MB");
        assert_eq!(size(999_990_000), "1.0 GB");
        assert_eq!(size(111_200_000_000), "111.2 GB");
    }

    #[test]
    fn ages_use_the_largest_whole_unit() {
        let day = Duration::from_secs(24 * 60 * 60);
        assert_eq!(age(day / 2), "today");
        assert_eq!(age(day), "1 day ago");
        assert_eq!(age(day * 45), "45 days ago");
        assert_eq!(age(day * 90), "3 months ago");
        assert_eq!(age(day * 400), "1 year ago");
        assert_eq!(age(day * 800), "2 years ago");
    }

    #[test]
    fn aligned_ages_line_up_the_ones_and_the_units() {
        let day = Duration::from_secs(24 * 60 * 60);
        let ages = [day / 2, day * 3, day * 39, day * 90, day * 400].map(age_aligned);
        assert_eq!(
            ages,
            [
                "   today",
                " 3 days ago",
                "39 days ago",
                " 3 months ago",
                " 1 year ago"
            ]
        );
    }

    #[test]
    fn counts_group_digits_in_threes() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1000), "1,000");
        assert_eq!(count(1_394_799), "1,394,799");
    }

    #[test]
    fn bars_and_percents_show_the_share() {
        assert_eq!(bar(0, 0, 12), "░".repeat(12));
        assert_eq!(
            bar(50, 100, 12),
            format!("{}{}", "█".repeat(6), "░".repeat(6))
        );
        assert_eq!(bar(100, 100, 12), "█".repeat(12));
        assert_eq!(bar(200, 100, 4), "████");
        assert_eq!(percent(1, 3), 33);
        assert_eq!(percent(5, 0), 0);
    }

    #[test]
    fn shortening_keeps_the_useful_ends() {
        assert_eq!(shorten_middle("film.mov", 20), "film.mov");
        assert_eq!(shorten_middle("a-very-long-name.dmg", 11), "a-ver…e.dmg");
        assert_eq!(shorten_middle("anything", 0), "");
        assert_eq!(shorten_path("~/Movies", 20), "~/Movies");
        assert_eq!(
            shorten_path("~/Library/Containers/com.docker.docker/Data", 32),
            "~/Library/…/Data"
        );
        // Too narrow to keep the start as well
        assert_eq!(shorten_path("~/Library/Containers/Docker", 12), "…/Docker");
        assert_eq!(shorten_path("anything", 1), "…");
    }

    #[test]
    fn shortening_respects_terminal_cells_and_graphemes() {
        use super::display_width;
        for text in [
            "日本語の長い名前.zip",
            "cafe\u{301}-backup.tar",
            "👩‍💻-project-backup.zip",
        ] {
            for width in 0..30 {
                for shortened in [shorten_middle(text, width), shorten_path(text, width)] {
                    assert!(
                        display_width(&shortened) <= width,
                        "{shortened:?} exceeds {width}"
                    );
                    assert!(!shortened.starts_with('\u{301}'));
                    assert!(!shortened.ends_with('\u{200d}'));
                }
            }
        }
        assert_eq!(shorten_middle("日本語.zip", 9), "日本….zip");
    }
}
