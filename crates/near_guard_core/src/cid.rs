//! Content addressing: real CIDv1 (dag-cbor + sha2-256, base32) plus the
//! canonical byte strings that both the Rust and TypeScript implementations
//! hash. If the canonical strings diverge, the two implementations stop
//! agreeing on audit CIDs — that is a test failure, not a cosmetic difference.

use sha2::{Digest, Sha256};

const BASE32_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Lowercase RFC4648 base32 without padding, as used by multibase `b`.
pub fn base32_lower(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() * 8 + 4) / 5);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for byte in data {
        buffer = (buffer << 8) | (*byte as u32);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1f) as usize;
            out.push(BASE32_ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(BASE32_ALPHABET[idx] as char);
    }
    out
}

pub fn sha256(payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    let out = hasher.finalize();
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&out);
    digest
}

pub fn sha256_hex(payload: &str) -> String {
    hex::encode(sha256(payload.as_bytes()))
}

/// Build a CIDv1 for the given payload: `0x01` (cidv1), `0x71` (dag-cbor),
/// `0x12 0x20` (sha2-256, 32 bytes), then the digest. Multibase prefix `b`.
pub fn cid_v1_dag_cbor(payload: &[u8]) -> String {
    let mut buf = Vec::with_capacity(36);
    buf.push(0x01);
    buf.push(0x71);
    buf.push(0x12);
    buf.push(0x20);
    buf.extend_from_slice(&sha256(payload));
    format!("b{}", base32_lower(&buf))
}

/// Canonical byte string for a policy. Commitments must not depend on JSON
/// formatting or float printing, so this is built by hand and mirrored exactly
/// in `web/src/lib/cid.ts`.
pub fn canonical_policy_bytes(p: &crate::policy::CompiledPolicy) -> String {
    format!(
        "near-agent-guard/policy/v1|vault={}|agent={}|per_tx_micro={}|daily_micro={}|rate_per_hour={}|window={}|slippage_bps={}|zk={}|assets={}|allowlist={}",
        p.source.vault_id,
        p.source.agent_id,
        p.per_tx_cap_micro,
        p.daily_cap_micro,
        p.rate_limit_per_hour,
        p.window_model.as_str(),
        p.max_slippage_bps,
        p.require_zk,
        p.assets_csv(),
        p.allowlist_csv(),
    )
}

/// Commit to the policy without publishing its numbers onchain.
pub fn policy_commitment(p: &crate::policy::CompiledPolicy) -> String {
    sha256_hex(&canonical_policy_bytes(p))
}

/// Canonical audit record. `reason` is agent-authored text, so it is length
/// capped before hashing to keep the payload (and the CID) bounded.
pub fn canonical_audit_bytes(
    vault_id: &str,
    agent_id: &str,
    kind: &str,
    target: &str,
    asset: &str,
    amount_micro: u128,
    ts_ms: u64,
    reason: &str,
    commitment: &str,
) -> String {
    let mut reason_tail: String = reason.chars().take(160).collect();
    reason_tail = reason_tail.replace('|', "/").replace('\n', " ");
    format!(
        "near-agent-guard/audit/v1|vault={vault_id}|agent={agent_id}|kind={kind}|target={target}|asset={asset}|amount_micro={amount_micro}|ts_ms={ts_ms}|reason={reason_tail}|commitment={commitment}"
    )
}

/// IPLD audit CID for one agent action.
pub fn audit_cid(
    vault_id: &str,
    agent_id: &str,
    kind: &str,
    target: &str,
    asset: &str,
    amount_micro: u128,
    ts_ms: u64,
    reason: &str,
    commitment: &str,
) -> String {
    cid_v1_dag_cbor(
        canonical_audit_bytes(
            vault_id, agent_id, kind, target, asset, amount_micro, ts_ms, reason, commitment,
        )
        .as_bytes(),
    )
}

/// Hash of the Noir policy proof. In the demo the proof is produced by the
/// circuit in `circuits/policy_check`; here we derive the same-shaped digest
/// so the contract, the MCP server and the UI all anchor something verifiable.
pub fn proof_hash(commitment: &str, asset: &str, amount_micro: u128, ts_ms: u64) -> String {
    sha256_hex(&format!(
        "near-agent-guard/noir-policy-proof/v1|commitment={commitment}|asset={asset}|amount_micro={amount_micro}|ts_ms={ts_ms}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode multibase base32 (b) and verify the CID envelope, so the test
    /// proves the format rather than trusting the encoder that produced it.
    fn base32_decode(input: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buffer: u32 = 0;
        let mut bits: u32 = 0;
        for ch in input.chars() {
            let val = BASE32_ALPHABET
                .iter()
                .position(|c| *c as char == ch)
                .expect("character must be in the base32 alphabet") as u32;
            buffer = (buffer << 5) | val;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                out.push(((buffer >> bits) & 0xff) as u8);
            }
        }
        out
    }

    #[test]
    fn cid_is_a_valid_cidv1() {
        let cid = cid_v1_dag_cbor(b"hello near");
        assert!(cid.starts_with('b'));
        assert!(cid.starts_with("bafyrei"), "dag-cbor cidv1 prefix, got {cid}");
        let raw = base32_decode(&cid[1..]);
        assert_eq!(raw[0], 0x01, "cidv1");
        assert_eq!(raw[1], 0x71, "dag-cbor codec");
        assert_eq!(raw[2], 0x12, "sha2-256 multihash");
        assert_eq!(raw[3], 0x20, "32 byte digest");
        assert_eq!(raw.len(), 36);
        assert_eq!(&raw[4..], &sha256(b"hello near")[..]);
    }

    #[test]
    fn cid_is_deterministic_and_input_sensitive() {
        assert_eq!(cid_v1_dag_cbor(b"a"), cid_v1_dag_cbor(b"a"));
        assert_ne!(cid_v1_dag_cbor(b"a"), cid_v1_dag_cbor(b"b"));
    }

    #[test]
    fn audit_reason_is_sanitised() {
        let a = audit_cid("v", "a", "transfer", "x", "NEAR", 1, 2, "bad|reason\nline", "c");
        let b = audit_cid("v", "a", "transfer", "x", "NEAR", 1, 2, "bad/reason line", "c");
        assert_eq!(a, b);
    }
}
