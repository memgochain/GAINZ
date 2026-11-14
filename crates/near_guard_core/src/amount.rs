//! Amount and time helpers.
//!
//! Money is handled as integer **micro-NEAR** (1 NEAR = 1_000_000 micro) so that
//! bounds, caps and fuzz arithmetic are exact and identical in Rust and in the
//! TypeScript mirror used by the Vercel MVP. Floating point never touches a
//! comparison.

/// 1 NEAR expressed in micro-NEAR.
pub const MICRO_PER_NEAR: u128 = 1_000_000;

/// Milliseconds in one day. Timestamps are milliseconds everywhere in this
/// crate so the JSON contracts stay inside JavaScript's safe integer range.
pub const MS_PER_DAY: u64 = 86_400_000;

/// Milliseconds in one hour.
pub const MS_PER_HOUR: u64 = 3_600_000;

/// Convert an authored NEAR amount (policy file or agent call) into micro-NEAR.
pub fn micro_from_near(near: f64) -> u128 {
    if !near.is_finite() || near <= 0.0 {
        return 0;
    }
    (near * MICRO_PER_NEAR as f64).round() as u128
}

/// Convert micro-NEAR back to NEAR for display and for authored JSON.
pub fn near_from_micro(micro: u128) -> f64 {
    micro as f64 / MICRO_PER_NEAR as f64
}

/// Human readable NEAR amount with trailing zeros trimmed: `5`, `1.5`, `0.000001`.
pub fn format_near(micro: u128) -> String {
    let whole = micro / MICRO_PER_NEAR;
    let frac = micro % MICRO_PER_NEAR;
    if frac == 0 {
        return whole.to_string();
    }
    let digits = format!("{:06}", frac);
    let trimmed = digits.trim_end_matches('0');
    format!("{}.{}", whole, trimmed)
}

/// Calendar day index for a millisecond timestamp.
pub fn day_index(ts_ms: u64) -> u64 {
    ts_ms / MS_PER_DAY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_micro_roundtrip() {
        assert_eq!(micro_from_near(1.5), 1_500_000);
        assert_eq!(micro_from_near(0.000001), 1);
        assert_eq!(micro_from_near(-3.0), 0);
        assert_eq!(format_near(10_000_000), "10");
        assert_eq!(format_near(1_500_000), "1.5");
        assert_eq!(format_near(1), "0.000001");
        assert_eq!(format_near(20_200_000), "20.2");
    }

    #[test]
    fn day_index_buckets() {
        let day_start = 20_001 * MS_PER_DAY;
        assert_eq!(day_index(day_start), 20_001);
        assert_eq!(day_index(day_start - 1), 20_000);
    }
}
