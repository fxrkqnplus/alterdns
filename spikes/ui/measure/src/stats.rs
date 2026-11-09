// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ölçüm istatistikleri.

/// En yakın sıra yöntemiyle yüzdelik (0–100). Boş girdide `None`.
pub fn percentile(values: &[f64], p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let rank = ((p / 100.0) * n as f64).ceil() as usize;
    Some(sorted[rank.clamp(1, n) - 1])
}

/// Ortanca (yüzdelik 50).
pub fn median(values: &[f64]) -> Option<f64> {
    percentile(values, 50.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_nearest_rank_of_known_series() {
        let v: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!(percentile(&v, 95.0), Some(19.0));
        assert_eq!(percentile(&v, 50.0), Some(10.0));
        assert_eq!(percentile(&v, 100.0), Some(20.0));
        assert_eq!(percentile(&v, 0.0), Some(1.0));
    }

    #[test]
    fn percentile_ignores_input_order() {
        let v = [5.0, 1.0, 4.0, 2.0, 3.0];
        assert_eq!(percentile(&v, 50.0), Some(3.0));
    }

    #[test]
    fn percentile_of_empty_is_none() {
        assert_eq!(percentile(&[], 95.0), None);
        assert_eq!(median(&[]), None);
    }
}
