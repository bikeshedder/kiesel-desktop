//! CSS-like transitions: a value eases towards its target over `--ksl-duration`.

use std::time::Instant;

/// `--ksl-ease` = Open Props `--ease-3`: cubic-bezier(.25, 0, .3, 1)
pub const EASE: CubicBezier = CubicBezier::new(0.25, 0.0, 0.3, 1.0);

#[derive(Clone, Copy, Debug)]
pub struct CubicBezier {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl CubicBezier {
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn sample(a1: f64, a2: f64, s: f64) -> f64 {
        // B(s) for P0 = 0, P3 = 1
        let r = 1.0 - s;
        3.0 * r * r * s * a1 + 3.0 * r * s * s * a2 + s * s * s
    }

    /// y for a given x (progress in time) – Newton steps, bisection as fallback
    pub fn ease(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        let mut s = x;
        for _ in 0..8 {
            let err = Self::sample(self.x1, self.x2, s) - x;
            if err.abs() < 1e-6 {
                return Self::sample(self.y1, self.y2, s);
            }
            let r = 1.0 - s;
            let d = 3.0 * r * r * self.x1
                + 6.0 * r * s * (self.x2 - self.x1)
                + 3.0 * s * s * (1.0 - self.x2);
            if d.abs() < 1e-6 {
                break;
            }
            s = (s - err / d).clamp(0.0, 1.0);
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..30 {
            s = (lo + hi) / 2.0;
            if Self::sample(self.x1, self.x2, s) < x {
                lo = s;
            } else {
                hi = s;
            }
        }
        Self::sample(self.y1, self.y2, s)
    }
}

/// A value in 0..=1 that transitions to a new target like a CSS transition:
/// from wherever it currently is, over the full duration
#[derive(Clone, Copy, Debug)]
pub struct Transition {
    from: f64,
    to: f64,
    start: Option<Instant>,
}

impl Transition {
    pub const fn new(value: f64) -> Self {
        Self {
            from: value,
            to: value,
            start: None,
        }
    }

    /// Returns true if the target changed (a transition starts)
    pub fn set(&mut self, to: f64, now: Instant, duration: f64) -> bool {
        if to == self.to {
            return false;
        }
        self.from = self.value(now, duration);
        self.to = to;
        self.start = Some(now);
        true
    }

    pub fn value(&self, now: Instant, duration: f64) -> f64 {
        let Some(start) = self.start else {
            return self.to;
        };
        let t = now.duration_since(start).as_secs_f64() / duration;
        if t >= 1.0 {
            return self.to;
        }
        self.from + (self.to - self.from) * EASE.ease(t)
    }

    pub fn animating(&self, now: Instant, duration: f64) -> bool {
        self.start
            .is_some_and(|s| now.duration_since(s).as_secs_f64() < duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_endpoints_and_monotonic() {
        assert!(EASE.ease(0.0).abs() < 1e-6);
        assert!((EASE.ease(1.0) - 1.0).abs() < 1e-6);
        let mut prev = 0.0;
        for i in 1..=100 {
            let y = EASE.ease(f64::from(i) / 100.0);
            assert!(y >= prev);
            prev = y;
        }
        // ease-3 is front-loaded: well past half-way at half the time
        assert!(EASE.ease(0.5) > 0.75);
    }

    #[test]
    fn transition_restarts_from_current_value() {
        let t0 = Instant::now();
        let mut tr = Transition::new(0.0);
        tr.set(1.0, t0, 0.2);
        let mid = t0 + std::time::Duration::from_millis(100);
        let v = tr.value(mid, 0.2);
        tr.set(0.0, mid, 0.2);
        assert!((tr.value(mid, 0.2) - v).abs() < 1e-9);
        assert_eq!(
            tr.value(mid + std::time::Duration::from_millis(300), 0.2),
            0.0
        );
    }
}
