//! Pure spending arithmetic for `GuardVault`.
//!
//! Everything here is a free function over `&[Spend]` and `NearToken`, with no
//! `env::` calls, so the rules that actually stop a drain can be tested on the
//! host target — no sandbox node, no TLS, no wasm. The contract's entry points
//! are thin wrappers around these functions.

use near_sdk::{near, NearToken};

/// One day in nanoseconds, the unit of `env::block_timestamp()`.
pub const DAY_IN_NS: u64 = 86_400_000_000_000;

/// How the daily spending window is measured.
#[near(serializers = [json, borsh])]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum WindowModel {
    /// Budget refills at 00:00 UTC. Two buckets fit in one 24h interval.
    Calendar,
    /// Any 24h sliding window. Costs a short spend history, cannot be doubled.
    Rolling,
}

/// A spend that still counts against the window.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Spend {
    pub ts_ns: u64,
    pub amount: NearToken,
}

/// Does a spend at `entry_ts` still count at `now_ts` under `model`?
pub fn within_window(entry_ts: u64, now_ts: u64, model: WindowModel) -> bool {
    match model {
        WindowModel::Rolling => entry_ts <= now_ts && now_ts.saturating_sub(entry_ts) < DAY_IN_NS,
        WindowModel::Calendar => entry_ts / DAY_IN_NS == now_ts / DAY_IN_NS,
    }
}

/// Sum of the spends that still count at `now_ns`. Saturates instead of
/// overflowing, so an absurd history can only ever look like "over the cap".
pub fn window_total(spends: &[Spend], now_ns: u64, model: WindowModel) -> NearToken {
    spends
        .iter()
        .filter(|spend| within_window(spend.ts_ns, now_ns, model))
        .fold(NearToken::from_yoctonear(0), |acc, spend| {
            acc.saturating_add(spend.amount)
        })
}

/// Drop spends that no longer count, keeping contract state bounded.
pub fn prune(spends: &mut Vec<Spend>, now_ns: u64, model: WindowModel) {
    spends.retain(|spend| within_window(spend.ts_ns, now_ns, model));
}

/// Budget left in the window. Saturates at zero: a vault can never report a
/// negative allowance, and an attacker cannot wrap the subtraction.
pub fn remaining(daily_limit: NearToken, spent: NearToken) -> NearToken {
    daily_limit.saturating_sub(spent)
}

/// Why a transfer was refused, if it was.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TransferCheck {
    /// Allowed; carries the window spend before this transfer.
    Ok { spent_before: NearToken },
    NonPositive,
    OverPerTx,
    OverWindow { spent_before: NearToken },
}

/// The arithmetic half of `execute_agent_transfer`, in the same order the
/// contract validates: positive amount, per-transaction cap, then the window.
pub fn check_transfer(
    spends: &[Spend],
    now_ns: u64,
    model: WindowModel,
    max_tx: NearToken,
    daily_limit: NearToken,
    amount: NearToken,
) -> TransferCheck {
    if amount.as_yoctonear() == 0 {
        return TransferCheck::NonPositive;
    }
    if amount.as_yoctonear() > max_tx.as_yoctonear() {
        return TransferCheck::OverPerTx;
    }
    let spent_before = window_total(spends, now_ns, model);
    if spent_before.saturating_add(amount).as_yoctonear() > daily_limit.as_yoctonear() {
        return TransferCheck::OverWindow { spent_before };
    }
    TransferCheck::Ok { spent_before }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 20_001 * DAY_IN_NS + 3_600_000_000_000; // day 20001, 01:00

    fn near(amount: u64) -> NearToken {
        NearToken::from_near(amount as u128)
    }

    fn spend_at(ts_ns: u64, amount_near: u64) -> Spend {
        Spend {
            ts_ns,
            amount: near(amount_near),
        }
    }

    fn limits() -> (NearToken, NearToken, WindowModel) {
        (near(5), near(10), WindowModel::Rolling)
    }

    // ----------------------------------------------------------- accumulation

    #[test]
    fn window_total_accumulates_spends_inside_the_window() {
        let spends = vec![
            spend_at(NOW - 1_000_000_000, 2),
            spend_at(NOW - 60_000_000_000, 5),
            spend_at(NOW, 1),
        ];
        assert_eq!(window_total(&spends, NOW, WindowModel::Rolling), near(8));
    }

    #[test]
    fn window_total_ignores_spends_outside_the_window() {
        let spends = vec![
            spend_at(NOW - DAY_IN_NS, 7),          // exactly 24h old: expired
            spend_at(NOW - DAY_IN_NS + 1, 3),      // one ns inside: counts
            spend_at(NOW - 2 * DAY_IN_NS, 100),    // ancient
        ];
        assert_eq!(window_total(&spends, NOW, WindowModel::Rolling), near(3));
    }

    #[test]
    fn window_total_saturates_on_absurd_history() {
        let spends = vec![
            Spend {
                ts_ns: NOW,
                amount: NearToken::from_yoctonear(u128::MAX),
            },
            Spend {
                ts_ns: NOW,
                amount: NearToken::from_yoctonear(u128::MAX),
            },
        ];
        assert_eq!(
            window_total(&spends, NOW, WindowModel::Rolling).as_yoctonear(),
            u128::MAX
        );
    }

    #[test]
    fn rolling_window_survives_midnight() {
        let day_start = 20_001 * DAY_IN_NS;
        let entry = day_start - 60_000_000_000; // 23:59 yesterday
        assert!(within_window(entry, day_start + 60_000_000_000, WindowModel::Rolling));
        assert!(!within_window(entry, day_start + 60_000_000_000, WindowModel::Calendar));
    }

    #[test]
    fn calendar_window_forgets_the_previous_day() {
        let day_start = 20_001 * DAY_IN_NS;
        let spends = vec![
            spend_at(day_start - 1, 10), // last ns of yesterday
            spend_at(day_start, 4),      // first ns of today
        ];
        assert_eq!(window_total(&spends, day_start + 60, WindowModel::Calendar), near(4));
    }

    // ---------------------------------------------------------------- pruning

    #[test]
    fn prune_keeps_only_spends_inside_the_window() {
        let mut spends = vec![
            spend_at(NOW - 2 * DAY_IN_NS, 9),
            spend_at(NOW - DAY_IN_NS, 8), // exactly 24h: dropped
            spend_at(NOW - DAY_IN_NS / 2, 2),
            spend_at(NOW, 1),
        ];
        prune(&mut spends, NOW, WindowModel::Rolling);
        assert_eq!(spends.len(), 2);
        assert_eq!(window_total(&spends, NOW, WindowModel::Rolling), near(3));
    }

    #[test]
    fn prune_uses_the_calendar_day_when_configured() {
        let day_start = 20_001 * DAY_IN_NS;
        let mut spends = vec![
            spend_at(day_start - 1, 10), // yesterday
            spend_at(day_start + 1, 4),  // today
            spend_at(day_start + 2, 6),  // today
        ];
        prune(&mut spends, day_start + 60_000_000_000, WindowModel::Calendar);
        assert_eq!(spends.len(), 2);
        assert_eq!(
            window_total(&spends, day_start + 60_000_000_000, WindowModel::Calendar),
            near(10)
        );
    }

    #[test]
    fn prune_is_idempotent() {
        let mut spends = vec![spend_at(NOW - 3 * DAY_IN_NS, 1), spend_at(NOW, 2)];
        prune(&mut spends, NOW, WindowModel::Rolling);
        let after_first: Vec<u64> = spends.iter().map(|s| s.ts_ns).collect();
        prune(&mut spends, NOW, WindowModel::Rolling);
        let after_second: Vec<u64> = spends.iter().map(|s| s.ts_ns).collect();
        assert_eq!(after_first, after_second);
        assert_eq!(spends.len(), 1);
    }

    // ------------------------------------------------------- remaining budget

    #[test]
    fn remaining_tracks_the_window_before_pruning() {
        let (_, daily, model) = limits();
        let spends = vec![spend_at(NOW - 1000, 3), spend_at(NOW - 2000, 4)];
        let spent = window_total(&spends, NOW, model);
        assert_eq!(remaining(daily, spent), near(3));
    }

    #[test]
    fn remaining_never_goes_negative() {
        assert_eq!(remaining(near(10), near(10)), NearToken::from_yoctonear(0));
        assert_eq!(remaining(near(10), near(25)), NearToken::from_yoctonear(0));
    }

    #[test]
    fn remaining_refills_when_the_window_moves() {
        let (_, daily, model) = limits();
        // One nanosecond inside the rolling window: still counts.
        let spends = vec![spend_at(NOW - DAY_IN_NS + 1, 10)];
        assert_eq!(
            remaining(daily, window_total(&spends, NOW, model)),
            NearToken::from_yoctonear(0)
        );

        // Two days later both models have forgotten it and the budget is back.
        let later = NOW + 2 * DAY_IN_NS;
        assert_eq!(remaining(daily, window_total(&spends, later, model)), near(10));
        assert_eq!(
            remaining(daily, window_total(&spends, later, WindowModel::Calendar)),
            near(10)
        );
    }

    // ------------------------------------------------------------ cap arithmetic

    #[test]
    fn transfer_check_rejects_zero_and_accepts_the_exact_cap() {
        let (max_tx, daily, model) = limits();
        let empty: Vec<Spend> = Vec::new();
        assert_eq!(
            check_transfer(&empty, NOW, model, max_tx, daily, NearToken::from_yoctonear(0)),
            TransferCheck::NonPositive
        );
        assert_eq!(
            check_transfer(&empty, NOW, model, max_tx, daily, near(5)),
            TransferCheck::Ok {
                spent_before: NearToken::from_yoctonear(0)
            }
        );
        assert_eq!(
            check_transfer(
                &empty,
                NOW,
                model,
                max_tx,
                daily,
                NearToken::from_yoctonear(near(5).as_yoctonear() + 1)
            ),
            TransferCheck::OverPerTx
        );
    }

    #[test]
    fn transfer_check_accumulates_until_the_window_is_full() {
        let (max_tx, daily, model) = limits();
        let mut spends: Vec<Spend> = Vec::new();

        // 5 + 5 = 10: the daily cap is reached exactly.
        for step in 0..2 {
            let check = check_transfer(&spends, NOW, model, max_tx, daily, near(5));
            assert!(
                matches!(check, TransferCheck::Ok { .. }),
                "spend {step} should be allowed"
            );
            spends.push(spend_at(NOW, 5));
        }

        // A third 5 NEAR payment is allowed by the per-tx cap but not the window.
        assert_eq!(
            check_transfer(&spends, NOW, model, max_tx, daily, near(5)),
            TransferCheck::OverWindow {
                spent_before: near(10)
            }
        );
    }

    #[test]
    fn transfer_check_allows_filling_the_window_exactly() {
        let (max_tx, daily, model) = limits();
        let spends = vec![spend_at(NOW - 1000, 4), spend_at(NOW - 2000, 3)];
        assert_eq!(
            check_transfer(&spends, NOW, model, max_tx, daily, near(3)),
            TransferCheck::Ok {
                spent_before: near(7)
            }
        );
        assert_eq!(
            check_transfer(
                &spends,
                NOW,
                model,
                max_tx,
                daily,
                NearToken::from_yoctonear(near(3).as_yoctonear() + 1)
            ),
            TransferCheck::OverWindow {
                spent_before: near(7)
            }
        );
    }

    #[test]
    fn transfer_check_ignores_expired_spends() {
        let (max_tx, daily, model) = limits();
        let spends = vec![spend_at(NOW - DAY_IN_NS, 10)]; // yesterday, rolling
        assert_eq!(
            check_transfer(&spends, NOW, model, max_tx, daily, near(5)),
            TransferCheck::Ok {
                spent_before: NearToken::from_yoctonear(0)
            }
        );
    }

    #[test]
    fn transfer_check_saturates_instead_of_panicking_on_overflow() {
        let (max_tx, daily, model) = limits();
        let spends = vec![Spend {
            ts_ns: NOW,
            amount: NearToken::from_yoctonear(u128::MAX),
        }];
        assert_eq!(
            check_transfer(&spends, NOW, model, max_tx, daily, near(1)),
            TransferCheck::OverWindow {
                spent_before: NearToken::from_yoctonear(u128::MAX)
            }
        );
    }

    #[test]
    fn per_tx_cap_is_checked_before_the_window() {
        let (max_tx, daily, model) = limits();
        let spends = vec![spend_at(NOW, 10)]; // window already exhausted
        assert_eq!(
            check_transfer(&spends, NOW, model, max_tx, daily, near(6)),
            TransferCheck::OverPerTx,
            "an over-cap amount reports the per-tx rule, not the window"
        );
    }
}
