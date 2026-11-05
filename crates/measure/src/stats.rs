//! Medians, percentiles and spreads.

/// The `p`th percentile of `values` (0 to 100), by nearest rank; `None` when there is none.
pub fn percentile(values: &[f64], p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((p / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    Some(sorted[rank.min(sorted.len()) - 1])
}

/// A figure over several runs: its median and its range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spread {
    pub median: f64,
    pub min: f64,
    pub max: f64,
}

impl Spread {
    /// The spread of `values`; `None` when there is none. The median of an even count is the mean
    /// of the two middle values.
    pub fn of(values: &[f64]) -> Option<Self> {
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let middle = sorted.len() / 2;
        let median = match sorted.len() {
            0 => return None,
            n if n % 2 == 0 => (sorted[middle - 1] + sorted[middle]) / 2.0,
            _ => sorted[middle],
        };
        Some(Self {
            median,
            min: values.iter().copied().fold(f64::INFINITY, f64::min),
            max: values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        })
    }

    /// `median (min–max)`, each with `decimals`.
    pub fn show(&self, decimals: usize) -> String {
        format!(
            "{:.d$} ({:.d$}–{:.d$})",
            self.median,
            self.min,
            self.max,
            d = decimals
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_by_nearest_rank() {
        let values: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&values, 50.0), Some(50.0));
        assert_eq!(percentile(&values, 99.0), Some(99.0));
        assert_eq!(percentile(&values, 100.0), Some(100.0));
        assert_eq!(percentile(&[3.0], 99.0), Some(3.0));
        assert_eq!(percentile(&[], 50.0), None);
    }

    #[test]
    fn a_spread_shows_its_median_and_range() {
        let spread = Spread::of(&[3.0, 1.0, 2.0]).unwrap();
        assert_eq!(spread.show(1), "2.0 (1.0–3.0)");
        assert_eq!(Spread::of(&[4.0, 1.0, 2.0, 3.0]).unwrap().median, 2.5);
        assert_eq!(Spread::of(&[]), None);
    }
}
