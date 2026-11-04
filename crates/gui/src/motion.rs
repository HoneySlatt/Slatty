//! Motion answering the pointer: a cover the pointer enters shows hovered by degrees, and goes
//! back the same way once left. Frames are drawn only while one of them moves.

use std::time::Duration;

use iced::Animation;
use iced::animation::Easing;
use iced::time::Instant;

/// Covers hovered, or left and still going back, by game id.
#[derive(Default)]
pub struct Hovers(Vec<(String, Animation<bool>)>);

impl Hovers {
    /// The pointer entered (`on`) or left the cover of `id` at `now`; one that went back is
    /// forgotten.
    pub fn set(&mut self, id: &str, on: bool, now: Instant, lasting: Duration) {
        self.0.retain(|(_, a)| a.value() || a.is_animating(now));
        match self.0.iter_mut().find(|(i, _)| i == id) {
            Some((_, a)) => a.go_mut(on, now),
            None if on => self.0.push((
                id.to_string(),
                Animation::new(false)
                    .duration(lasting)
                    .easing(Easing::EaseOutCubic)
                    .go(true, now),
            )),
            None => {}
        }
    }

    /// How hovered the cover of `id` shows at `now`, from 0 to 1; `None` when the pointer was not
    /// seen entering it (the games under it changed, not the pointer).
    pub fn shown(&self, id: &str, now: Instant) -> Option<f32> {
        self.0
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, a)| a.interpolate(0.0, 1.0, now))
    }

    /// Whether the cover of `id` was left and is still going back at `now`.
    pub fn leaving(&self, id: &str, now: Instant) -> bool {
        self.0
            .iter()
            .any(|(i, a)| i == id && !a.value() && a.is_animating(now))
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.0.iter().any(|(_, a)| a.is_animating(now))
    }

    /// Forgets them all: the page they were on is gone.
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LASTING: Duration = Duration::from_millis(100);

    #[test]
    fn a_cover_shows_hovered_by_degrees_and_goes_back_once_left() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let mut hovers = Hovers::default();
        let shown = |hovers: &Hovers, ms| hovers.shown("1", at(ms)).unwrap();
        hovers.set("1", true, start, LASTING);
        assert_eq!(shown(&hovers, 0), 0.0);
        let half = shown(&hovers, 50);
        assert!(half > 0.0 && half < 1.0, "{half}");
        assert_eq!(shown(&hovers, 100), 1.0);
        assert!(hovers.animating(at(50)) && !hovers.animating(at(100)));
        assert_eq!(hovers.shown("2", at(50)), None, "another cover");

        hovers.set("1", false, at(200), LASTING);
        assert!(hovers.leaving("1", at(250)));
        assert!(shown(&hovers, 250) < 1.0);
        assert!(!hovers.leaving("1", at(300)));
        assert_eq!(shown(&hovers, 300), 0.0);

        // Gone back, it is forgotten when another cover is entered.
        hovers.set("2", true, at(400), LASTING);
        assert_eq!(hovers.0.len(), 1);
    }

    /// Left halfway, a cover goes back from where it was rather than jumping to fully hovered.
    #[test]
    fn a_cover_left_halfway_goes_back_from_where_it_was() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let mut hovers = Hovers::default();
        hovers.set("1", true, start, LASTING);
        let before = hovers.shown("1", at(40)).unwrap();
        hovers.set("1", false, at(40), LASTING);
        let after = hovers.shown("1", at(41)).unwrap();
        assert!((after - before).abs() < 0.1, "{before} then {after}");
    }

    #[test]
    fn without_motion_a_cover_shows_hovered_at_once() {
        let now = Instant::now();
        let mut hovers = Hovers::default();
        hovers.set("1", true, now, Duration::ZERO);
        assert_eq!(hovers.shown("1", now), Some(1.0));
        assert!(!hovers.animating(now));
        hovers.set("1", false, now, Duration::ZERO);
        assert_eq!(hovers.shown("1", now), Some(0.0));
        assert!(!hovers.leaving("1", now));
    }
}
