//! Motion answering the pointer and what opens: a cover the pointer enters shows hovered by
//! degrees, and goes back the same way once left; a drawer or a dialog comes into place; key art
//! fades into the next. Frames are drawn only while one of them moves.

use std::time::Duration;

use iced::Animation;
use iced::animation::Easing;
use iced::time::Instant;
use iced::widget::image::Handle;

/// Covers hovered (or given the couch mode's focus), or left and still going back, by game id.
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

    /// The cover the pointer (or the focus) is on, if any.
    pub fn on(&self) -> Option<&str> {
        self.0
            .iter()
            .find(|(_, a)| a.value())
            .map(|(i, _)| i.as_str())
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

/// Drawers and dialogs coming into place, each by the layer it is in: one opening over another
/// leaves that one still.
#[derive(Default)]
pub struct Arrivals(Vec<(&'static str, Animation<bool>)>);

impl Arrivals {
    /// What the layer `layer` shows changed to something new at `now`: it comes into place.
    pub fn open(&mut self, layer: &'static str, now: Instant, lasting: Duration) {
        self.0.retain(|(l, a)| *l != layer && a.is_animating(now));
        self.0.push((
            layer,
            Animation::new(false)
                .duration(lasting)
                .easing(Easing::EaseOutCubic)
                .go(true, now),
        ));
    }

    /// How far what `layer` shows has come into place at `now`, from 0 to 1.
    pub fn shown(&self, layer: &str, now: Instant) -> f32 {
        self.0
            .iter()
            .find(|(l, _)| *l == layer)
            .map_or(1.0, |(_, a)| a.interpolate(0.0, 1.0, now))
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.0.iter().any(|(_, a)| a.is_animating(now))
    }
}

/// A picture giving way to the next: the new one fades in over the one before, which stays under
/// it until it is done; to nothing (while the next downloads), the one before fades out.
#[derive(Default)]
pub struct Crossfade {
    shown: Option<Handle>,
    before: Option<Handle>,
    change: Option<Animation<bool>>,
}

impl Crossfade {
    /// `art` shows from `now`: a change from what showed starts the fade, and says so.
    pub fn show(&mut self, art: Option<Handle>, now: Instant, lasting: Duration) -> bool {
        if art == self.shown {
            return false;
        }
        self.before = self.shown.take();
        self.shown = art;
        self.change = Some(
            Animation::new(false)
                .duration(lasting)
                .easing(Easing::EaseOutCubic)
                .go(true, now),
        );
        true
    }

    /// The pictures to draw at `now`, the lowest first, each with its opacity.
    pub fn layers(&self, now: Instant) -> Vec<(&Handle, f32)> {
        let done = self
            .change
            .as_ref()
            .map_or(1.0, |a| a.interpolate(0.0, 1.0, now));
        let mut layers = Vec::with_capacity(2);
        if let Some(before) = &self.before
            && done < 1.0
        {
            let left = if self.shown.is_some() {
                1.0
            } else {
                1.0 - done
            };
            layers.push((before, left));
        }
        if let Some(shown) = &self.shown {
            layers.push((shown, done));
        }
        layers
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.change.as_ref().is_some_and(|a| a.is_animating(now))
    }

    /// Forgets what showed: the next picture shows at once, as the page it is on arrives.
    pub fn clear(&mut self) {
        *self = Crossfade::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LASTING: Duration = Duration::from_millis(100);

    #[test]
    fn a_picture_fades_in_over_the_one_before_or_out_to_nothing() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let (a, b) = (
            Handle::from_path("/[FAKE]/a.jpg"),
            Handle::from_path("/[FAKE]/b.jpg"),
        );
        let opacities = |fade: &Crossfade, ms| -> Vec<(Handle, f32)> {
            fade.layers(at(ms))
                .into_iter()
                .map(|(h, o)| (h.clone(), o))
                .collect()
        };
        let mut fade = Crossfade::default();
        fade.show(Some(a.clone()), start, LASTING);
        fade.show(Some(a.clone()), at(100), LASTING);
        assert_eq!(
            opacities(&fade, 100),
            [(a.clone(), 1.0)],
            "the same: no change"
        );

        fade.show(Some(b.clone()), at(200), LASTING);
        let half = opacities(&fade, 250);
        assert_eq!(half.len(), 2);
        assert_eq!(
            half[0],
            (a.clone(), 1.0),
            "the one before stays whole under it"
        );
        assert!(half[1].0 == b && half[1].1 > 0.0 && half[1].1 < 1.0);
        assert_eq!(opacities(&fade, 300), [(b.clone(), 1.0)]);
        assert!(fade.animating(at(250)) && !fade.animating(at(300)));

        fade.show(None, at(400), LASTING);
        let going = opacities(&fade, 450);
        assert!(going.len() == 1 && going[0].0 == b && going[0].1 < 1.0);
        assert!(opacities(&fade, 500).is_empty());
    }

    #[test]
    fn what_opens_comes_into_place_and_leaves_the_others_still() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let mut arrivals = Arrivals::default();
        assert_eq!(arrivals.shown("drawer", start), 1.0, "nothing opened");
        arrivals.open("drawer", start, LASTING);
        assert_eq!(arrivals.shown("drawer", start), 0.0);
        let half = arrivals.shown("drawer", at(50));
        assert!(half > 0.0 && half < 1.0, "{half}");
        arrivals.open("dialog", at(50), LASTING);
        assert_eq!(arrivals.shown("drawer", at(50)), half, "the drawer goes on");
        assert_eq!(arrivals.shown("dialog", at(50)), 0.0);
        assert!(arrivals.animating(at(120)));
        assert!(!arrivals.animating(at(150)));
        assert_eq!(arrivals.shown("dialog", at(150)), 1.0);
        // Done, they are forgotten when the next opens.
        arrivals.open("drawer", at(200), LASTING);
        assert_eq!(arrivals.0.len(), 1);
    }

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
