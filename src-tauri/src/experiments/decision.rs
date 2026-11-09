//! Deterministic node-cpu-v1 decisions. Durations are integer nanoseconds.
#![allow(dead_code)]
//! Comparisons use cross-multiplication, not floating point.

pub const SPREAD_NUMERATOR: u128 = 2;
pub const SPREAD_DENOMINATOR: u128 = 100;
pub const GAIN_NUMERATOR: u128 = 3;
pub const GAIN_DENOMINATOR: u128 = 100;
pub const MAX_SAMPLE_NS: u128 = 60_000_000_000;
pub const CALIBRATION_TARGET_NS: u128 = 50_000_000;
pub const MAX_BATCH: u32 = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sample {
    Ns(u128),
    Invalid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Judgment {
    Keep,
    Discard { reason: &'static str },
    Inconclusive { reason: &'static str },
    Invalid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaselineGate {
    Ready,
    Unstable,
    Invalid,
}

pub fn baseline_gate(samples: &[Sample]) -> BaselineGate {
    let Ok(values) = finite_five(samples) else {
        return BaselineGate::Invalid;
    };
    match spread(&values) {
        Some((num, den)) if spread_within_limit(num, den) => BaselineGate::Ready,
        Some(_) => BaselineGate::Unstable,
        None => BaselineGate::Invalid,
    }
}

pub fn judge(best: &[Sample], candidate: &[Sample]) -> Judgment {
    let (best, candidate) = match (finite_five(best), finite_five(candidate)) {
        (Ok(best), Ok(candidate)) => (best, candidate),
        _ => return Judgment::Invalid,
    };
    let Some((best_spread_num, best_spread_den)) = spread(&best) else {
        return Judgment::Invalid;
    };
    let Some((candidate_spread_num, candidate_spread_den)) = spread(&candidate) else {
        return Judgment::Invalid;
    };
    let Some((noise_num, noise_den)) = wider_spread(
        (best_spread_num, best_spread_den),
        (candidate_spread_num, candidate_spread_den),
    ) else {
        return Judgment::Invalid;
    };
    if spread_exceeds_limit(noise_num, noise_den) {
        return Judgment::Inconclusive {
            reason: "unstable_samples",
        };
    }
    let Some((gain_num, gain_den)) = median_ratio(&candidate, &best) else {
        return Judgment::Invalid;
    };
    // g = 1 - C/B = (B - C) / B. Non-positive gain is no improvement.
    if gain_num >= gain_den {
        return Judgment::Discard {
            reason: "no_improvement",
        };
    }
    let improvement = gain_den - gain_num;
    match compare_gain(improvement, gain_den, noise_num, noise_den) {
        Some(GainCompare::AtLeastThreshold) => Judgment::Keep,
        Some(GainCompare::BelowThreshold) => Judgment::Discard {
            reason: "no_improvement",
        },
        Some(GainCompare::Overlap) => Judgment::Inconclusive {
            reason: "threshold_overlap",
        },
        None => Judgment::Invalid,
    }
}

/// Uncertain and failure streaks. A conclusive keep or discard clears uncertainty.
pub fn advance_streaks(
    uncertain: u32,
    failures: u32,
    judgment: &Judgment,
) -> (u32, u32, Option<&'static str>) {
    match judgment {
        Judgment::Keep | Judgment::Discard { .. } => (0, failures, None),
        Judgment::Inconclusive { .. } => {
            let uncertain = uncertain.saturating_add(1);
            let stop = (uncertain >= 3).then_some("measurement_uncertain");
            (uncertain, failures, stop)
        }
        Judgment::Invalid => {
            let failures = failures.saturating_add(1);
            let stop = (failures >= 3).then_some("measurement_failed");
            (uncertain, failures, stop)
        }
    }
}

pub fn next_batch_size(current: u32, elapsed_ns: u128) -> Result<u32, &'static str> {
    if current == 0 || current > MAX_BATCH || !current.is_power_of_two() {
        return Err("template_incompatible");
    }
    if elapsed_ns >= CALIBRATION_TARGET_NS {
        return Ok(current);
    }
    if current == MAX_BATCH {
        return Err("template_incompatible");
    }
    Ok(current.saturating_mul(2))
}

fn finite_five(samples: &[Sample]) -> Result<[u128; 5], ()> {
    if samples.len() != 5 {
        return Err(());
    }
    let mut values = [0; 5];
    for (slot, sample) in values.iter_mut().zip(samples) {
        match sample {
            Sample::Ns(value) if *value > 0 && *value <= MAX_SAMPLE_NS => *slot = *value,
            _ => return Err(()),
        }
    }
    Ok(values)
}

fn spread(values: &[u128; 5]) -> Option<(u128, u128)> {
    let mut ordered = *values;
    ordered.sort_unstable();
    let median = ordered[2];
    if median == 0 {
        return None;
    }
    let mut widest = 0u128;
    for value in ordered {
        widest = widest.max(value.abs_diff(median));
    }
    Some((widest, median))
}

fn wider_spread(left: (u128, u128), right: (u128, u128)) -> Option<(u128, u128)> {
    let left_scaled = left.0.checked_mul(right.1)?;
    let right_scaled = right.0.checked_mul(left.1)?;
    if left_scaled >= right_scaled {
        Some(left)
    } else {
        Some(right)
    }
}

fn spread_exceeds_limit(numerator: u128, denominator: u128) -> bool {
    numerator
        .checked_mul(SPREAD_DENOMINATOR)
        .zip(denominator.checked_mul(SPREAD_NUMERATOR))
        .is_some_and(|(left, right)| left > right)
}

fn spread_within_limit(numerator: u128, denominator: u128) -> bool {
    numerator
        .checked_mul(SPREAD_DENOMINATOR)
        .zip(denominator.checked_mul(SPREAD_NUMERATOR))
        .is_some_and(|(left, right)| left <= right)
}

fn median_ratio(candidate: &[u128; 5], best: &[u128; 5]) -> Option<(u128, u128)> {
    let mut ratios = [(0u128, 0u128); 5];
    for index in 0..5 {
        if best[index] == 0 {
            return None;
        }
        ratios[index] = (candidate[index], best[index]);
    }
    ratios.sort_by(|left, right| {
        let left_scaled = left.0.saturating_mul(right.1);
        let right_scaled = right.0.saturating_mul(left.1);
        left_scaled.cmp(&right_scaled)
    });
    let (numerator, denominator) = ratios[2];
    if numerator.checked_mul(ratios[0].1).is_none()
        || denominator.checked_mul(ratios[0].0).is_none()
    {
        return None;
    }
    Some((numerator, denominator))
}

enum GainCompare {
    AtLeastThreshold,
    BelowThreshold,
    Overlap,
}

fn compare_gain(
    improvement: u128,
    base: u128,
    noise_num: u128,
    noise_den: u128,
) -> Option<GainCompare> {
    let noise_term = noise_num.checked_mul(2)?.checked_mul(GAIN_DENOMINATOR)?;
    let gain_term = GAIN_NUMERATOR.checked_mul(noise_den)?;
    let scale = GAIN_DENOMINATOR.checked_mul(noise_den)?;
    let upper = gain_term.checked_add(noise_term)?;
    let lower = gain_term.checked_sub(noise_term);
    let left = improvement.checked_mul(scale)?;
    let keep_right = base.checked_mul(upper)?;
    if left >= keep_right {
        return Some(GainCompare::AtLeastThreshold);
    }
    let Some(lower) = lower else {
        return Some(GainCompare::Overlap);
    };
    let discard_right = base.checked_mul(lower)?;
    if left < discard_right {
        Some(GainCompare::BelowThreshold)
    } else {
        Some(GainCompare::Overlap)
    }
}

pub fn ns(milliseconds: u128) -> Sample {
    Sample::Ns(milliseconds.saturating_mul(1_000_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(values: &[u128]) -> Vec<Sample> {
        values.iter().copied().map(ns).collect()
    }

    #[test]
    fn decision_table_matches_the_frozen_rule() {
        let cases = [
            (vec![100; 5], vec![90; 5], Judgment::Keep),
            (vec![100; 5], vec![97; 5], Judgment::Keep),
            (
                vec![100; 5],
                vec![98; 5],
                Judgment::Discard {
                    reason: "no_improvement",
                },
            ),
            (
                vec![100; 5],
                vec![100; 5],
                Judgment::Discard {
                    reason: "no_improvement",
                },
            ),
            (
                vec![100; 5],
                vec![105; 5],
                Judgment::Discard {
                    reason: "no_improvement",
                },
            ),
            (
                vec![99, 100, 101, 100, 100],
                vec![96; 5],
                Judgment::Inconclusive {
                    reason: "threshold_overlap",
                },
            ),
            (
                vec![98, 100, 102, 100, 100],
                vec![98, 100, 102, 100, 100],
                Judgment::Discard {
                    reason: "no_improvement",
                },
            ),
            (
                vec![95, 100, 105, 100, 100],
                vec![90; 5],
                Judgment::Inconclusive {
                    reason: "unstable_samples",
                },
            ),
        ];
        for (best, candidate, expected) in cases {
            assert_eq!(
                judge(&row(&best), &row(&candidate)),
                expected,
                "{best:?} vs {candidate:?}"
            );
        }
        assert_eq!(
            judge(
                &row(&[100; 5]),
                &[ns(100), ns(100), ns(100), ns(100), Sample::Ns(0)]
            ),
            Judgment::Invalid
        );
        assert_eq!(judge(&row(&[100; 4]), &row(&[90; 5])), Judgment::Invalid);
        assert_eq!(
            baseline_gate(&row(&[95, 100, 105, 100, 100])),
            BaselineGate::Unstable
        );
        assert_eq!(baseline_gate(&row(&[100; 5])), BaselineGate::Ready);
        assert_eq!(baseline_gate(&[Sample::Invalid; 5]), BaselineGate::Invalid);
    }

    #[test]
    fn calibration_picks_the_first_batch_at_least_50ms_and_stops_at_the_cap() {
        assert_eq!(next_batch_size(1, 49_000_000).unwrap(), 2);
        assert_eq!(next_batch_size(2, 50_000_000).unwrap(), 2);
        assert_eq!(
            next_batch_size(65_536, 49_000_000),
            Err("template_incompatible")
        );
        assert_eq!(next_batch_size(3, 50_000_000), Err("template_incompatible"));
    }

    #[test]
    fn conclusive_results_reset_uncertainty_and_three_inconclusive_results_stop() {
        let (uncertain, failures, stop) = advance_streaks(
            0,
            0,
            &Judgment::Inconclusive {
                reason: "threshold_overlap",
            },
        );
        assert_eq!((uncertain, failures, stop), (1, 0, None));
        let (uncertain, _, stop) = advance_streaks(
            2,
            0,
            &Judgment::Inconclusive {
                reason: "unstable_samples",
            },
        );
        assert_eq!((uncertain, stop), (3, Some("measurement_uncertain")));
        let (uncertain, failures, _) = advance_streaks(2, 1, &Judgment::Keep);
        assert_eq!((uncertain, failures), (0, 1));
        let (_, failures, stop) = advance_streaks(0, 2, &Judgment::Invalid);
        assert_eq!((failures, stop), (3, Some("measurement_failed")));
    }
}
