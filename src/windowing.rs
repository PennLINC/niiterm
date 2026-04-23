use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use ndarray::ArrayView3;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowMode {
    Percentile(f32, f32),
    Raw(f32, f32),
    Full,
}

impl WindowMode {
    pub fn label(self) -> String {
        match self {
            Self::Percentile(lo, hi) => format!("p{:.0},p{:.0}", lo, hi),
            Self::Raw(lo, hi) => format!("{:.1},{:.1}", lo, hi),
            Self::Full => "full".to_string(),
        }
    }
}

impl FromStr for WindowMode {
    type Err = WindowParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("full") {
            return Ok(Self::Full);
        }

        let parts = s.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(WindowParseError::BadFormat);
        }

        let is_percentile = parts
            .iter()
            .all(|part| part.starts_with('p') || part.starts_with('P'));
        if is_percentile {
            let lo = parts[0][1..]
                .parse::<f32>()
                .map_err(|_| WindowParseError::BadNumber)?;
            let hi = parts[1][1..]
                .parse::<f32>()
                .map_err(|_| WindowParseError::BadNumber)?;
            return Ok(Self::Percentile(lo, hi));
        }

        let lo = parts[0]
            .parse::<f32>()
            .map_err(|_| WindowParseError::BadNumber)?;
        let hi = parts[1]
            .parse::<f32>()
            .map_err(|_| WindowParseError::BadNumber)?;
        Ok(Self::Raw(lo, hi))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub lo: f32,
    pub hi: f32,
}

impl Window {
    pub fn clamp(self) -> Self {
        if self.hi <= self.lo {
            Self {
                lo: self.lo,
                hi: self.lo + 1.0,
            }
        } else {
            self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowPreset {
    P1P99,
    P2P98,
    P5P95,
    Full,
}

impl WindowPreset {
    pub const ALL: [Self; 4] = [Self::P1P99, Self::P2P98, Self::P5P95, Self::Full];

    pub fn to_mode(self) -> WindowMode {
        match self {
            Self::P1P99 => WindowMode::Percentile(1.0, 99.0),
            Self::P2P98 => WindowMode::Percentile(2.0, 98.0),
            Self::P5P95 => WindowMode::Percentile(5.0, 95.0),
            Self::Full => WindowMode::Full,
        }
    }
}

#[derive(Debug, Error)]
pub enum WindowParseError {
    #[error("window should be `pLO,pHI`, `LO,HI`, or `full`")]
    BadFormat,
    #[error("window values must be numeric")]
    BadNumber,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CacheKey {
    volume: usize,
    mode_bits: (u32, u32, u8),
}

#[derive(Debug, Default)]
pub struct WindowCache {
    cache: HashMap<CacheKey, Window>,
}

impl WindowCache {
    pub fn get_or_insert(
        &mut self,
        volume_index: usize,
        mode: WindowMode,
        volume: ArrayView3<'_, f32>,
    ) -> Window {
        let key = CacheKey {
            volume: volume_index,
            mode_bits: match mode {
                WindowMode::Percentile(lo, hi) => (lo.to_bits(), hi.to_bits(), 0),
                WindowMode::Raw(lo, hi) => (lo.to_bits(), hi.to_bits(), 1),
                WindowMode::Full => (0, 0, 2),
            },
        };

        if let Some(window) = self.cache.get(&key).copied() {
            return window;
        }

        let computed = compute_window(volume, mode);
        self.cache.insert(key, computed);
        computed
    }
}

pub fn compute_window(volume: ArrayView3<'_, f32>, mode: WindowMode) -> Window {
    match mode {
        WindowMode::Raw(lo, hi) => Window { lo, hi }.clamp(),
        WindowMode::Full => full_range(volume),
        WindowMode::Percentile(lo, hi) => percentile_range(volume, lo, hi),
    }
}

pub fn full_range(volume: ArrayView3<'_, f32>) -> Window {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;

    for value in volume.iter().copied().filter(|v| v.is_finite()) {
        min = min.min(value);
        max = max.max(value);
    }

    if !min.is_finite() || !max.is_finite() {
        Window { lo: 0.0, hi: 1.0 }
    } else {
        Window { lo: min, hi: max }.clamp()
    }
}

fn percentile_range(volume: ArrayView3<'_, f32>, lo: f32, hi: f32) -> Window {
    let mut sample = subsample_finite(volume);
    if sample.is_empty() {
        return Window { lo: 0.0, hi: 1.0 };
    }

    sample.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lo_val = percentile(&sample, lo / 100.0);
    let hi_val = percentile(&sample, hi / 100.0);
    Window {
        lo: lo_val,
        hi: hi_val,
    }
    .clamp()
}

fn subsample_finite(volume: ArrayView3<'_, f32>) -> Vec<f32> {
    let finite_count = volume.iter().filter(|v| v.is_finite()).count();
    if finite_count == 0 {
        return Vec::new();
    }

    let step = (finite_count / 100_000).max(1);
    volume
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .step_by(step)
        .collect()
}

fn percentile(sorted: &[f32], q: f32) -> f32 {
    if sorted.len() == 1 {
        return sorted[0];
    }

    let q = q.clamp(0.0, 1.0);
    let pos = q * (sorted.len().saturating_sub(1) as f32);
    let lower = pos.floor() as usize;
    let upper = pos.ceil() as usize;

    if lower == upper {
        sorted[lower]
    } else {
        let weight = pos - lower as f32;
        sorted[lower] * (1.0 - weight) + sorted[upper] * weight
    }
}

impl fmt::Display for WindowMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array3;

    #[test]
    fn parses_window_specs() {
        assert_eq!(
            "p2,p98".parse::<WindowMode>().unwrap(),
            WindowMode::Percentile(2.0, 98.0)
        );
        assert_eq!(
            "0,100".parse::<WindowMode>().unwrap(),
            WindowMode::Raw(0.0, 100.0)
        );
        assert_eq!("full".parse::<WindowMode>().unwrap(), WindowMode::Full);
    }

    #[test]
    fn computes_range_for_small_arrays() {
        let arr = Array3::from_shape_vec((2, 2, 2), vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0])
            .unwrap();
        let window = compute_window(arr.view(), WindowMode::Percentile(25.0, 75.0));
        assert!(window.lo >= 1.0);
        assert!(window.hi <= 6.0);
    }
}
