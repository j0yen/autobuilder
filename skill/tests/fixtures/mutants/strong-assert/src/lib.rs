//! AC1 fixture: a function whose single test pins the exact return value.
//!
//! cargo-mutants will try replacing the body with constants (e.g. `0`, `1`,
//! `-1`) and with `i32::MAX` etc. Because the test asserts strict equality to
//! `2`, every viable value mutation makes the test fail and is therefore
//! CAUGHT. Expected outcome: mutation_kill_rate == 1.0.

/// Returns the constant `2`.
pub fn two() -> i32 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_is_exactly_two() {
        assert_eq!(two(), 2);
    }
}
