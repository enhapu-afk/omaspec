//! A chart's axes as numbers: the range each runs, as written or from the
//! values, and its ticks with their words.

use super::Axis;
use super::read::MOST_TICKS;

/// An axis's range, and its ticks with their words.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Scale {
    pub low: f64,
    pub high: f64,
    pub ticks: Vec<(f64, String)>,
}

impl Scale {
    /// The scale of `axis`, its range from `values` where none is written.
    pub fn of(axis: &Axis, values: &[f64]) -> Scale {
        let (low, high) = axis.range.unwrap_or_else(|| range_of(values));
        Scale {
            low,
            high,
            ticks: ticks(axis, low, high),
        }
    }

    /// Where `value` lies along the axis: 0 at its low end, 1 at its high.
    pub fn at(&self, value: f64) -> f32 {
        ((value - self.low) / (self.high - self.low)) as f32
    }

    /// Where a bar stands: at 0, or at the end of the range nearer to it.
    pub fn base(&self) -> f64 {
        0.0_f64.clamp(self.low, self.high)
    }

    /// Whether 0 lies inside the range, not at an end: a line marks it.
    pub fn crosses_zero(&self) -> bool {
        self.low < 0.0 && self.high > 0.0
    }
}

/// The range of `values` where none is written: 0 to 100 where they lie
/// within it, as PlantUML has it, else from 0, or a round number below
/// the least where it is less, to a round number above the most.
pub(super) fn range_of(values: &[f64]) -> (f64, f64) {
    let least = values.iter().copied().fold(0.0, f64::min);
    let most = values.iter().copied().fold(0.0, f64::max);
    if least >= 0.0 && most <= 100.0 {
        return (0.0, 100.0);
    }
    let (low, high) = (round_out(least), round_out(most));
    if low < high && (high - low).is_finite() {
        (low, high)
    } else if least < most {
        (least, most)
    } else {
        (least - 1.0, least + 1.0)
    }
}

/// `value` rounded away from 0 to 1, 2, 2.5 or 5 times a power of ten.
fn round_out(value: f64) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return value;
    }
    let power = 10_f64.powf(value.abs().log10().floor());
    let mantissa = value.abs() / power;
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .into_iter()
        .find(|step| *step >= mantissa - 1e-9)
        .unwrap_or(10.0);
    let rounded = (step * power).copysign(value);
    if rounded.is_finite() { rounded } else { value }
}

/// The ticks of an axis from `low` to `high`: those written with their
/// words, else a step of `spacing` from the low end, or five even steps
/// as PlantUML takes without it.
pub(super) fn ticks(axis: &Axis, low: f64, high: f64) -> Vec<(f64, String)> {
    if !axis.ticks.is_empty() {
        return axis
            .ticks
            .iter()
            .filter(|(value, _)| (low..=high).contains(value))
            .cloned()
            .collect();
    }
    let even = (high - low) / 5.0;
    let step = axis
        .spacing
        .filter(|step| (high - low) / step <= MOST_TICKS)
        .unwrap_or(even);
    if !(step.is_finite() && step > 0.0) {
        return vec![(low, said(low)), (high, said(high))];
    }
    let mut out = Vec::new();
    for at in 0.. {
        let value = low + at as f64 * step;
        if value > high + step * 1e-9 || at as f64 > MOST_TICKS {
            break;
        }
        out.push((value, said(value)));
    }
    out
}

/// A value as a chart writes it: no needless decimals, and 0 never as -0.
pub(super) fn said(value: f64) -> String {
    let said = crate::chart::said(value);
    if said == "-0" { "0".to_owned() } else { said }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis(range: Option<(f64, f64)>, spacing: Option<f64>) -> Axis {
        Axis {
            range,
            spacing,
            ..Axis::default()
        }
    }

    #[test]
    fn an_axis_ticks_as_plantuml_does() {
        // Five even steps without a spacing, as PlantUML takes them.
        let words =
            |scale: Scale| -> Vec<String> { scale.ticks.into_iter().map(|t| t.1).collect() };
        assert_eq!(
            words(Scale::of(&axis(Some((0.0, 140.0)), None), &[])),
            ["0", "28", "56", "84", "112", "140"]
        );
        assert_eq!(
            words(Scale::of(&axis(Some((-10.0, 10.0)), Some(5.0)), &[])),
            ["-10", "-5", "0", "5", "10"]
        );
        // Without a range: 0 to 100 where the values lie in it, else round
        // numbers around them.
        assert_eq!(range_of(&[3.0, 7.0]), (0.0, 100.0));
        assert_eq!(range_of(&[120.0, 7.0]), (0.0, 200.0));
        assert_eq!(range_of(&[-30.0, 70.0]), (-50.0, 100.0));
        assert_eq!(range_of(&[1.7e308, 1.7e308]), (0.0, 1.7e308));
        let written = Axis {
            ticks: vec![
                (0.0, "Poor".into()),
                (50.0, "Average".into()),
                (200.0, "x".into()),
            ],
            ..axis(Some((0.0, 100.0)), None)
        };
        assert_eq!(words(Scale::of(&written, &[])), ["Poor", "Average"]);
        let scale = Scale::of(&axis(Some((-50.0, 50.0)), None), &[]);
        assert!(scale.crosses_zero() && scale.base() == 0.0 && scale.at(0.0) == 0.5);
    }
}
