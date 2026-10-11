//! Sizes and speeds as shown to the user, in the units Steam shows: decimal kilobytes, megabytes,
//! gigabytes and terabytes (1 MB = 1,000,000 bytes), and network speeds in megabits a second.

/// "440.3 MB", "59.3 GB".
pub fn size(bytes: u64) -> String {
    let b = bytes as f64;
    match bytes {
        1_000_000_000_000.. => format!("{:.1} TB", b / 1e12),
        1_000_000_000.. => format!("{:.1} GB", b / 1e9),
        1_000_000.. => format!("{:.1} MB", b / 1e6),
        _ => format!("{} KB", bytes / 1000),
    }
}

/// A speed in bytes a second, in bits a second: "120.4 Mbps".
pub fn speed(bytes_per_second: f64) -> String {
    let bits = bytes_per_second * 8.0;
    if bits >= 1e6 {
        format!("{:.1} Mbps", bits / 1e6)
    } else {
        format!("{:.0} Kbps", bits / 1e3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_speeds_read_as_steam_shows_them() {
        assert_eq!(size(440_300_000), "440.3 MB");
        assert_eq!(size(59_300_000_000), "59.3 GB");
        assert_eq!(size(2_400_000_000_000), "2.4 TB");
        assert_eq!(size(512_000), "512 KB");
        assert_eq!(size(0), "0 KB");
        assert_eq!(speed(15_050_000.0), "120.4 Mbps");
        assert_eq!(speed(50_000.0), "400 Kbps");
    }
}
