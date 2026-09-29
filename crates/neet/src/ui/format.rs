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

/// `part` as a whole percent of `total`, rounded down.
pub fn percent(part: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    u64::try_from(u128::from(part) * 100 / u128::from(total)).unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::{bar, count, percent, size};

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
}
