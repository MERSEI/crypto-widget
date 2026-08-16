use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Rolling-window spike detector: compares the latest price to the oldest price still
/// inside `window`. Returns the signed percent change when `|change| >= threshold_pct`.
pub fn detect(buffer: &VecDeque<(Instant, f64)>, window: Duration, threshold_pct: f64) -> Option<f64> {
    let (latest_at, latest_price) = *buffer.back()?;
    let (_, oldest_price) = *buffer
        .iter()
        .find(|(t, _)| latest_at.duration_since(*t) <= window)?;
    if oldest_price == 0.0 {
        return None;
    }
    let change_pct = (latest_price - oldest_price) / oldest_price * 100.0;
    if change_pct.abs() >= threshold_pct {
        Some(change_pct)
    } else {
        None
    }
}

/// Drops points older than `max_age` so the buffer doesn't grow unbounded over a long session.
pub fn trim(buffer: &mut VecDeque<(Instant, f64)>, max_age: Duration) {
    let now = Instant::now();
    while let Some((t, _)) = buffer.front() {
        if now.duration_since(*t) > max_age {
            buffer.pop_front();
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf_from(points: &[(u64, f64)]) -> VecDeque<(Instant, f64)> {
        let base = Instant::now();
        points
            .iter()
            .map(|(secs_ago, price)| (base - Duration::from_secs(*secs_ago), *price))
            .collect()
    }

    #[test]
    fn fires_on_large_move_within_window() {
        let buf = buf_from(&[(600, 100.0), (300, 101.0), (0, 104.0)]);
        let result = detect(&buf, Duration::from_secs(900), 3.0);
        assert!(result.is_some());
        assert!((result.unwrap() - 4.0).abs() < 0.01);
    }

    #[test]
    fn silent_below_threshold() {
        let buf = buf_from(&[(600, 100.0), (0, 101.0)]);
        assert!(detect(&buf, Duration::from_secs(900), 3.0).is_none());
    }

    #[test]
    fn ignores_points_outside_window() {
        // Only the 100.0 -> 104.0 move over the last 300s is in-window; the older 90.0 point
        // must not be used as the baseline.
        let buf = buf_from(&[(1200, 90.0), (300, 100.0), (0, 104.0)]);
        let result = detect(&buf, Duration::from_secs(600), 3.0).unwrap();
        assert!((result - 4.0).abs() < 0.01);
    }

    #[test]
    fn empty_buffer_is_silent() {
        let buf: VecDeque<(Instant, f64)> = VecDeque::new();
        assert!(detect(&buf, Duration::from_secs(900), 3.0).is_none());
    }

    #[test]
    fn trim_drops_old_points_only() {
        let mut buf = buf_from(&[(7200, 1.0), (10, 2.0), (0, 3.0)]);
        trim(&mut buf, Duration::from_secs(3600));
        assert_eq!(buf.len(), 2);
    }
}
