//! Private spending-policy proof.
//!
//! The agent proves, without revealing the limits, that:
//!   * the policy it is following is the one the owner committed to,
//!   * the amount is positive,
//!   * `spent_today + tx_amount <= secret_daily_limit`,
//!   * the recipient hash is well formed.
//!
//! Public inputs are the policy commitment and a recipient hash (the storage
//! vs. data location matches what `GuardVault` anchors onchain). The daily limit
//! and the salt stay private.

use std::hash::pedersen_hash;

fn main(
    tx_amount: u64,
    spent_today: u64,
    secret_daily_limit: u64,
    policy_salt: Field,
    public_policy_commitment: pub Field,
    public_recipient_hash: pub Field,
) {
    let computed_commitment = pedersen_hash([secret_daily_limit as Field, policy_salt]);
    assert(computed_commitment == public_policy_commitment, "invalid policy commitment");

    assert(tx_amount > 0, "transfer amount must be positive");

    let total_spent = spent_today + tx_amount;
    assert(total_spent <= secret_daily_limit, "ZK policy violation: daily limit exceeded");

    assert(public_recipient_hash != 0, "invalid recipient hash");
}

#[test]
fn accepts_spend_inside_the_limit() {
    let limit: u64 = 100;
    let salt: Field = 123456789;
    let commitment = pedersen_hash([limit as Field, salt]);

    main(25, 50, limit, salt, commitment, 999);
}

#[test(should_fail)]
fn rejects_spend_over_the_limit() {
    let limit: u64 = 100;
    let salt: Field = 123456789;
    let commitment = pedersen_hash([limit as Field, salt]);

    main(75, 50, limit, salt, commitment, 999);
}

#[test(should_fail)]
fn rejects_wrong_policy_commitment() {
    let limit: u64 = 100;
    let salt: Field = 123456789;
    let commitment = pedersen_hash([limit as Field, salt + 1]);

    main(10, 0, limit, salt, commitment, 999);
}

#[test(should_fail)]
fn rejects_zero_amount() {
    let limit: u64 = 100;
    let salt: Field = 123456789;
    let commitment = pedersen_hash([limit as Field, salt]);

    main(0, 0, limit, salt, commitment, 999);
}
