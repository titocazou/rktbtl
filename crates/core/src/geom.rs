//! Small 2D geometry helpers. Plain `f64`, no allocation, all `#[inline]` so the
//! hot loop stays branch-light and the optimizer can vectorize across envs.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn len(self) -> f64 {
        self.x.hypot(self.y)
    }

    #[inline]
    pub fn len_sq(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    #[inline]
    pub fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }

    #[inline]
    pub fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }

    #[inline]
    pub fn scale(self, s: f64) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }

    #[inline]
    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }

    #[inline]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// Squared distance between two line segments `[a0,a1]` and `[b0,b1]`.
///
/// Returns the squared minimum distance together with the closest point on
/// segment A. This is the workhorse for capsule-vs-capsule rocket collisions;
/// returning the closest point on A lets the caller decide *which half* of
/// rocket A was involved in the contact (top = indestructible, bottom = fatal).
pub fn seg_seg_closest(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> (f64, Vec2) {
    // Classic Ericson "Real-Time Collision Detection" closest-point-of-segments.
    let d1 = a1.sub(a0); // direction of segment A
    let d2 = b1.sub(b0); // direction of segment B
    let r = a0.sub(b0);
    let aa = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);

    const EPS: f64 = 1e-12;
    let (mut s, mut t);

    if aa <= EPS && e <= EPS {
        // both segments are points
        s = 0.0;
        t = 0.0;
    } else if aa <= EPS {
        // segment A is a point
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= EPS {
            // segment B is a point
            t = 0.0;
            s = (-c / aa).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = aa * e - b * b;
            s = if denom > EPS {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / aa).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / aa).clamp(0.0, 1.0);
            }
        }
    }

    let cp_a = a0.add(d1.scale(s));
    let cp_b = b0.add(d2.scale(t));
    (cp_a.sub(cp_b).len_sq(), cp_a)
}

/// Squared distance from point `p` to segment `[a,b]`.
pub fn point_seg_dist_sq(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b.sub(a);
    let t = if ab.len_sq() <= 1e-12 {
        0.0
    } else {
        (p.sub(a).dot(ab) / ab.len_sq()).clamp(0.0, 1.0)
    };
    p.sub(a.add(ab.scale(t))).len_sq()
}
