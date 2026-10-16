//! Realtime chart: the last 10 minutes of total read / write rates, one point per
//! sample (about a second). The view shows the last 60 or all 600 points.

use std::collections::VecDeque;

use serde::Serialize;

/// Points kept: 10 minutes at one sample per second.
pub const CAPACITY: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Point {
    /// Unix time, seconds.
    pub t: u64,
    /// Bytes / s.
    pub read: f32,
    pub write: f32,
}

#[derive(Debug, Default)]
pub struct Chart {
    points: VecDeque<Point>,
}

impl Chart {
    /// Add a point; a second sample within the same second replaces the first.
    pub fn push(&mut self, p: Point) {
        if self.points.back().is_some_and(|b| b.t == p.t) {
            self.points.pop_back();
        }
        self.points.push_back(p);
        while self.points.len() > CAPACITY {
            self.points.pop_front();
        }
    }

    /// Points of the last `seconds` seconds (by time, not count: a pause leaves a gap).
    pub fn last(&self, seconds: usize) -> Vec<Point> {
        let Some(end) = self.points.back().map(|p| p.t) else {
            return Vec::new();
        };
        let from = end.saturating_sub(seconds.saturating_sub(1) as u64);
        self.points
            .iter()
            .filter(|p| p.t >= from)
            .copied()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(t: u64) -> Point {
        Point {
            t,
            read: t as f32,
            write: 0.0,
        }
    }

    #[test]
    fn keeps_ten_minutes_and_slices_by_time() {
        let mut c = Chart::default();
        for t in 0..700 {
            c.push(p(t));
        }
        assert_eq!(c.len(), CAPACITY);
        let last = c.last(60);
        assert_eq!(last.len(), 60);
        assert_eq!(last[0].t, 640);
        assert_eq!(c.last(600).len(), 600);
    }

    #[test]
    fn same_second_replaces_and_gaps_stay_gaps() {
        let mut c = Chart::default();
        c.push(p(10));
        c.push(Point {
            t: 10,
            read: 99.0,
            write: 0.0,
        });
        assert_eq!(c.len(), 1);
        assert_eq!(c.last(60)[0].read, 99.0);
        c.push(p(200)); // after a pause
        assert_eq!(c.last(60).len(), 1);
        assert!(Chart::default().last(60).is_empty());
    }
}
