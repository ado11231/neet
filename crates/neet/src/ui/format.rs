use std::time::Duration;

use ratatui::style::Stylize;
use ratatui::text::Span;

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
    const DAY: u64 = 24 * 60 * 60;
    let days = elapsed.as_secs() / DAY;
    let (value, unit) = match days {
        0 => return "today".to_string(),
        1..=59 => (days, "day"),
        60..=364 => (days / 30, "month"),
        _ => (days / 365, "year"),
    };
    let plural = if value == 1 { "" } else { "s" };
    format!("{value} {unit}{plural} ago")
}

/// `part` as a whole percent of `total`, rounded down.
pub fn percent(part: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    u64::try_from(u128::from(part) * 100 / u128::from(total)).unwrap_or(100)
}

/// Shortens `text` to `width` characters by cutting out its middle, so a
/// file name keeps its start and its extension.
pub fn shorten_middle(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let keep = width - 1;
    let end = keep / 2;
    let start = keep - end;
    let head: String = chars[..start].iter().collect();
    let tail: String = chars[chars.len() - end..].iter().collect();
    format!("{head}…{tail}")
}

/// Shortens a folder path to `width` characters by cutting out its middle.
/// Keeps where it starts, such as `~/Library`, and the part nearest the file.
pub fn shorten_path(path: &str, width: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= width {
        return path.to_string();
    }
    if width == 0 {
        return String::new();
    }
    // The first two parts, such as `~/Library/`, when they leave room for the end
    let head: String = path.split_inclusive('/').take(2).collect();
    let head_len = head.chars().count();
    let head = if head_len < chars.len() && head_len + 1 + 12 <= width {
        head
    } else {
        String::new()
    };
    let keep = width - head.chars().count() - 1;
    let tail: String = chars[chars.len() - keep..].iter().collect();
    // Start the end at a whole folder name when there is one
    let tail = match tail.find('/') {
        Some(slash) if slash + 1 < tail.len() => tail[slash..].to_string(),
        _ => tail,
    };
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::{age, bar, count, percent, shorten_middle, shorten_path, size};
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
}
