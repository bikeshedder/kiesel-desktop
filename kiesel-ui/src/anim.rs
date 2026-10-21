//! CSS-like transitions: a value eases towards its target over `--ksl-duration`.

use std::time::Instant;

/// `--ksl-ease` = Open Props `--ease-3`: cubic-bezier(.25, 0, .3, 1)
pub const EASE: CubicBezier = CubicBezier::new(0.25, 0.0, 0.3, 1.0);

/// `--ksl-ease-spring` = Open Props `--ease-spring-3`, a CSS `linear()` curve that
/// overshoots to 1.16 and settles: (output, input position if given)
const SPRING_3: &[(f64, Option<f64>)] = &[
    (0.0, None),
    (0.009, None),
    (0.035, Some(0.021)),
    (0.141, Some(0.044)),
    (0.723, Some(0.129)),
    (0.938, Some(0.167)),
    (1.017, None),
    (1.077, None),
    (1.121, None),
    (1.149, Some(0.243)),
    (1.159, None),
    (1.163, None),
    (1.161, None),
    (1.154, Some(0.299)),
    (1.129, Some(0.328)),
    (1.051, Some(0.396)),
    (1.017, Some(0.431)),
    (0.991, None),
    (0.977, Some(0.51)),
    (0.974, Some(0.538)),
    (0.975, Some(0.571)),
    (0.997, Some(0.698)),
    (1.003, Some(0.769)),
    (1.0, None),
];

/// How a transition's progress maps to its value
#[derive(Clone, Copy, Debug)]
pub enum Easing {
    Cubic(CubicBezier),
    /// CSS `linear()`: points (output, input), straight lines between them
    Linear(&'static [(f64, Option<f64>)]),
}

impl Easing {
    pub const EASE: Self = Self::Cubic(EASE);
    pub const SPRING: Self = Self::Linear(SPRING_3);

    pub fn ease(&self, x: f64) -> f64 {
        match self {
            Self::Cubic(c) => c.ease(x),
            Self::Linear(points) => linear(points, x.clamp(0.0, 1.0)),
        }
    }
}

/// CSS `linear()`: the first point sits at 0 and the last at 1 unless given;
/// points without a position are spread evenly between their neighbours
fn linear(points: &[(f64, Option<f64>)], x: f64) -> f64 {
    let n = points.len();
    let mut at: Vec<Option<f64>> = points.iter().map(|p| p.1).collect();
    at[0] = Some(at[0].unwrap_or(0.0));
    at[n - 1] = Some(at[n - 1].unwrap_or(1.0));
    let mut i = 0;
    while i < n {
        if at[i].is_some() {
            i += 1;
            continue;
        }
        let (prev, mut next) = (i - 1, i);
        while at[next].is_none() {
            next += 1;
        }
        let (a, b) = (at[prev].unwrap(), at[next].unwrap());
        for (k, slot) in at.iter_mut().enumerate().take(next).skip(i) {
            *slot = Some(a + (b - a) * (k - prev) as f64 / (next - prev) as f64);
        }
        i = next;
    }
    for w in 0..n - 1 {
        let (x0, x1) = (at[w].unwrap(), at[w + 1].unwrap());
        if x <= x1 {
            let t = if x1 > x0 { (x - x0) / (x1 - x0) } else { 1.0 };
            return points[w].0 + (points[w + 1].0 - points[w].0) * t;
        }
    }
    points[n - 1].0
}

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
    easing: Easing,
}

impl Transition {
    pub const fn new(value: f64) -> Self {
        Self {
            from: value,
            to: value,
            start: None,
            easing: Easing::EASE,
        }
    }

    pub const fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// The value it is heading to
    pub fn target(&self) -> f64 {
        self.to
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
        if duration <= 0.0 {
            return self.to;
        }
        let t = now.duration_since(start).as_secs_f64() / duration;
        if t >= 1.0 {
            return self.to;
        }
        self.from + (self.to - self.from) * self.easing.ease(t)
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
    fn spring_overshoots_and_settles() {
        let s = Easing::SPRING;
        assert_eq!(s.ease(0.0), 0.0);
        assert_eq!(s.ease(1.0), 1.0);
        // the unpositioned points are spread out: 1.017, 1.077, 1.121 fall between 16.7 % and 24.3 %
        assert!((s.ease(0.186) - 1.017).abs() < 1e-9);
        assert!((s.ease(0.275) - 1.163).abs() < 1e-3, "the peak");
        assert!((s.ease(0.129) - 0.723).abs() < 1e-9);
    }

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
