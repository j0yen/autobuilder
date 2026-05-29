//! AC2 fixture: a function with a deliberately weak test assertion.
//!
//! `five()` returns 5, but the test only checks `> 0`. cargo-mutants replaces
//! the body with `1` (also `> 0`), and that mutant SURVIVES the test — exactly
//! the false-green failure mode this PRD exists to surface. Expected outcome:
//! mutants_alive_count >= 1, mutation_kill_rate < 1.0.

/// Returns `5`.
pub fn five() -> i32 {
    5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_is_positive() {
        // Intentionally weak: does not pin the value, only its sign.
        assert!(five() > 0);
    }
}
