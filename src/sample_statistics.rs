// Copyright (C) 2020, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0

/// Owned scalar samples, separate from a Node's operational counters.
///
/// Preserves Gazebo's population deviation, extrema sentinels and comparison
/// order, including signed zero and nonfinite samples. Storage is constant.
#[derive(Clone, Copy, Debug)]
pub struct SampleStatistics {
    count: u64,
    average: f64,
    sum_square_mean_dist: f64,
    min: f64,
    max: f64,
}

impl SampleStatistics {
    pub const fn new() -> Self {
        Self {
            count: 0,
            average: 0.0,
            sum_square_mean_dist: 0.0,
            min: f64::MAX,
            max: f64::MIN_POSITIVE,
        }
    }

    /// Add a sample, including nonfinite values.
    ///
    /// The unsigned count wraps as in native, including its zero denominator at
    /// exhaustion. No sample is rejected or silently omitted from the count.
    pub fn update(&mut self, value: f64) {
        self.count = self.count.wrapping_add(1);
        let previous_average = self.average;
        self.average = previous_average + (value - previous_average) / self.count as f64;
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        // Keep unnormalized M2 and the native operation order. Storing variance
        // between updates loses nonzero results at the subnormal boundary.
        self.sum_square_mean_dist += (value - previous_average) * (value - self.average);
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn avg(&self) -> f64 {
        self.average
    }

    /// Population standard deviation; zero before the first sample.
    pub fn std_dev(&self) -> f64 {
        if self.count > 0 {
            (self.sum_square_mean_dist / self.count as f64).sqrt()
        } else {
            0.0
        }
    }

    pub fn min(&self) -> f64 {
        self.min
    }

    /// Native initializes this to f64::MIN_POSITIVE, even for nonpositive samples.
    pub fn max(&self) -> f64 {
        self.max
    }
}

impl Default for SampleStatistics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../tests/support/sample_statistics.rs"]
mod tests;
