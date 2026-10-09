//! Which CDN endpoint to download from. GOG lists several, and the first one can be many times
//! slower than another from a given network (measured: 0.8 MB/s against 13 MB/s), so each one is
//! tried once and chunks then go to the fastest.
//!
//! Most chunks arrive in about a second, but a few take ten times longer (measured: 0.6 to 1.5 s,
//! then 6 to 11 s). Once a request lasts well beyond the usual time on its endpoint, the chunk is
//! asked again and the first answer wins.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

#[derive(Default)]
pub(super) struct Speeds {
    seen: HashMap<String, Speed>,
    picks: usize,
}

#[derive(Default, Clone)]
struct Speed {
    /// Smoothed bytes per second; zero after a failure.
    rate: Option<f64>,
    /// A first request is under way, so others do not all wait on an unknown endpoint.
    probing: bool,
    in_flight: usize,
    /// Durations of the latest successful requests, in seconds.
    recent: VecDeque<f64>,
}

impl Speeds {
    /// Index of the endpoint to use among `names`, leaving out `avoid` unless nothing else is left:
    /// one not measured yet if any is free, else the fastest measured (the second fastest once in
    /// [`EXPLORE`] picks, so an endpoint that recovers from a stall is noticed), else the least busy.
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
        let mut measured: Vec<(usize, f64)> = candidates
            .iter()
            .copied()
            .filter_map(|i| Some((i, self.seen.get(names[i])?.rate?)))
            .collect();
        measured.sort_by(|a, b| b.1.total_cmp(&a.1));
        self.picks += 1;
        let rank = usize::from(self.picks.is_multiple_of(EXPLORE) && measured.len() > 1);
        let i = measured
            .get(rank)
            .map(|&(i, _)| i)
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
        if s.recent.len() == RECENT {
            s.recent.pop_front();
        }
        s.recent.push_back(took.as_secs_f64());
    }

    /// How long to wait on a request to `name` before asking another copy: three times its usual
    /// request time, at least two seconds; `None` until a few requests have been timed.
    pub(super) fn hedge_after(&self, name: &str) -> Option<Duration> {
        let s = self.seen.get(name)?;
        if s.recent.len() < 4 {
            return None;
        }
        let mut sorted: Vec<f64> = s.recent.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        Some(Duration::from_secs_f64(
            (3.0 * sorted[sorted.len() / 2]).max(2.0),
        ))
    }

    /// A request still running when its copy, `bytes` long, arrived: the endpoint is at least that
    /// slow for now, so following chunks leave it while it stays stalled.
    pub(super) fn outrun(&mut self, name: &str, bytes: usize, waited: Duration) {
        let slow = bytes as f64 / waited.as_secs_f64().max(0.001);
        if let Some(s) = self.seen.get_mut(name) {
            s.rate = Some(s.rate.map_or(slow, |old| 0.7 * old + 0.3 * slow).min(slow));
            s.probing = false;
            s.in_flight = s.in_flight.saturating_sub(1);
        }
    }

    /// A request dropped because another copy answered first.
    pub(super) fn abandoned(&mut self, name: &str) {
        if let Some(s) = self.seen.get_mut(name) {
            s.probing = false;
            s.in_flight = s.in_flight.saturating_sub(1);
        }
    }

    pub(super) fn failed(&mut self, name: &str) {
        let s = self.seen.entry(name.to_string()).or_default();
        s.rate = Some(0.0);
        s.probing = false;
        s.in_flight = s.in_flight.saturating_sub(1);
    }
}

const RECENT: usize = 16;
const EXPLORE: usize = 32;

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
        let picks: Vec<usize> = (0..64).map(|_| s.pick(&NAMES, &[])).collect();
        assert_eq!(picks.iter().filter(|&&i| i == 1).count(), 62);
        assert_eq!(
            picks.iter().filter(|&&i| i == 2).count(),
            2,
            "the runner-up is checked now and then"
        );
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

    #[test]
    fn a_copy_is_asked_after_three_usual_request_times() {
        let mut s = Speeds::default();
        for secs in [1, 1, 1] {
            s.record("gcore", 10_000_000, Duration::from_secs(secs));
        }
        assert_eq!(
            s.hedge_after("gcore"),
            None,
            "not enough requests timed yet"
        );
        s.record("gcore", 10_000_000, Duration::from_secs(11));
        assert_eq!(
            s.hedge_after("gcore"),
            Some(Duration::from_secs(3)),
            "one straggler does not move the usual time"
        );
        for _ in 0..4 {
            s.record("fastly", 10_000, Duration::from_millis(100));
        }
        assert_eq!(s.hedge_after("fastly"), Some(Duration::from_secs(2)));
    }

    #[test]
    fn an_abandoned_copy_frees_its_slot() {
        let mut s = Speeds::default();
        assert_eq!(s.pick(&NAMES, &[]), 0);
        s.abandoned("fastly");
        assert_eq!(
            s.pick(&NAMES, &[]),
            0,
            "still unmeasured and no longer probing"
        );
    }

    #[test]
    fn a_stalled_endpoint_is_left_until_it_recovers() {
        let mut s = Speeds::default();
        s.record("fastly", 15_000_000, SECOND);
        s.record("gcore", 8_000_000, SECOND);
        assert_eq!(s.pick(&NAMES[..2], &[]), 0);
        s.outrun("fastly", 10_000_000, Duration::from_secs(4));
        assert_eq!(
            s.pick(&NAMES[..2], &[]),
            1,
            "copy from gcore came first: fastly is stalled"
        );
        for _ in 0..20 {
            s.record("fastly", 15_000_000, SECOND);
        }
        assert_eq!(
            s.pick(&NAMES[..2], &[]),
            0,
            "back once its requests are fast again"
        );
    }
}
