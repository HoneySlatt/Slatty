//! Which CDN endpoint to download from. GOG lists several, and the first one can be many times
//! slower than another from a given network (measured: 0.8 MB/s against 13 MB/s), so each one is
//! tried once and chunks then go to the fastest.

use std::collections::HashMap;
use std::time::Duration;

#[derive(Default)]
pub(super) struct Speeds {
    seen: HashMap<String, Speed>,
}

#[derive(Default, Clone, Copy)]
struct Speed {
    /// Smoothed bytes per second; zero after a failure.
    rate: Option<f64>,
    /// A first request is under way, so others do not all wait on an unknown endpoint.
    probing: bool,
    in_flight: usize,
}

impl Speeds {
    /// Index of the endpoint to use among `names`, leaving out `avoid` unless nothing else is left:
    /// one not measured yet if any is free, else the fastest measured, else the least busy.
    pub(super) fn pick(&mut self, names: &[&str], avoid: &[usize]) -> usize {
        let mut candidates: Vec<usize> = (0..names.len()).filter(|i| !avoid.contains(i)).collect();
        if candidates.is_empty() {
            candidates = (0..names.len()).collect();
        }
        if let Some(&i) = candidates.iter().find(|&&i| {
            self.seen
                .get(names[i])
                .is_none_or(|s| s.rate.is_none() && !s.probing)
        }) {
            self.start(names[i]).probing = true;
            return i;
        }
        let i = candidates
            .iter()
            .copied()
            .filter_map(|i| Some((i, self.seen.get(names[i])?.rate?)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
            .or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .min_by_key(|&i| self.seen.get(names[i]).map_or(0, |s| s.in_flight))
            })
            .unwrap_or(0);
        self.start(names[i]);
        i
    }

    fn start(&mut self, name: &str) -> &mut Speed {
        let s = self.seen.entry(name.to_string()).or_default();
        s.in_flight += 1;
        s
    }

    pub(super) fn record(&mut self, name: &str, bytes: usize, took: Duration) {
        let now = bytes as f64 / took.as_secs_f64().max(0.001);
        let s = self.seen.entry(name.to_string()).or_default();
        s.rate = Some(s.rate.map_or(now, |old| 0.7 * old + 0.3 * now));
        s.probing = false;
        s.in_flight = s.in_flight.saturating_sub(1);
    }

    pub(super) fn failed(&mut self, name: &str) {
        let s = self.seen.entry(name.to_string()).or_default();
        s.rate = Some(0.0);
        s.probing = false;
        s.in_flight = s.in_flight.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: [&str; 3] = ["fastly", "gcore", "akamai"];
    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn each_endpoint_is_tried_once_then_the_fastest_wins() {
        let mut s = Speeds::default();
        assert_eq!(s.pick(&NAMES, &[]), 0);
        assert_eq!(
            s.pick(&NAMES, &[]),
            1,
            "no second request waits on an unknown one"
        );
        assert_eq!(s.pick(&NAMES, &[]), 2);
        assert_eq!(s.pick(&NAMES, &[]), 0, "all probing: the least busy");
        assert_eq!(s.pick(&NAMES, &[]), 1);
        assert_eq!(s.pick(&NAMES, &[]), 2);
        s.record("fastly", 1_000_000, SECOND);
        s.record("gcore", 13_000_000, SECOND);
        s.record("akamai", 5_000_000, SECOND);
        for _ in 0..5 {
            assert_eq!(s.pick(&NAMES, &[]), 1);
        }
    }

    #[test]
    fn a_slowing_endpoint_loses_its_place() {
        let mut s = Speeds::default();
        s.record("fastly", 6_000_000, SECOND);
        s.record("gcore", 8_000_000, SECOND);
        s.record("akamai", 1_000_000, SECOND);
        assert_eq!(s.pick(&NAMES, &[]), 1);
        s.record("gcore", 1_000_000, SECOND);
        s.record("gcore", 1_000_000, SECOND);
        assert_eq!(s.pick(&NAMES, &[]), 0);
    }

    #[test]
    fn a_failed_endpoint_is_avoided_but_not_when_it_is_the_last_one() {
        let mut s = Speeds::default();
        s.record("fastly", 9_000_000, SECOND);
        s.record("gcore", 2_000_000, SECOND);
        s.record("akamai", 1_000_000, SECOND);
        assert_eq!(
            s.pick(&NAMES, &[0]),
            1,
            "this chunk already failed on fastly"
        );
        s.failed("fastly");
        assert_eq!(s.pick(&NAMES, &[]), 1);
        assert_eq!(s.pick(&NAMES[..1], &[0]), 0);
    }
}
