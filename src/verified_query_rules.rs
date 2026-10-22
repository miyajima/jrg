//! Reviewed approvals bind aliases to an exact normalized request and engine.
//! Unseen questions, English, budgets or documents use caller English alone.
use crate::selection::SelectionRequest;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, sync::OnceLock};

const REGISTRY: &str = include_str!("../data/verified-query-rules.v1.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    schema_version: u32,
    version: String,
    engine_sha256: String,
    catalog_sha256: String,
    approvals: Vec<Approval>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    id: String,
    request_sha256: String,
    rule_ids: Vec<String>,
    evidence: Evidence,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    path: String,
    sha256: String,
    baseline_units: Vec<String>,
    selected_units: Vec<String>,
}

pub fn request_fingerprint(request: &SelectionRequest) -> Result<String, String> {
    let bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("request exceeds 4 MiB".into());
    }
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Source identity excludes approvals, so recording a proof cannot approve new code.
pub fn engine_identity() -> String {
    let mut hash = Sha256::new();
    for (name, source) in [
        ("selection", include_str!("selection.rs")),
        ("expansion", include_str!("query_expansion.rs")),
        ("ranking", include_str!("rank.rs")),
        ("budget", include_str!("budget.rs")),
        ("adapters", include_str!("adapters.rs")),
        ("normalization", include_str!("normalize.rs")),
        ("approval", include_str!("verified_query_rules.rs")),
        ("cli", include_str!("bin/jrg-select.rs")),
        ("dependencies", include_str!("../Cargo.toml")),
        ("dependency-lock", include_str!("../Cargo.lock")),
    ] {
        hash.update(name.as_bytes());
        hash.update((source.len() as u64).to_le_bytes());
        hash.update(source.as_bytes());
    }
    format!("{hash:x}", hash = hash.finalize())
}

pub fn policy_info() -> serde_json::Value {
    serde_json::json!({
        "default_mode": "verified",
        "engine_sha256": engine_identity(),
        "catalog_sha256": crate::query_expansion::catalog_sha256(),
        "registry_sha256": format!("{:x}", Sha256::digest(REGISTRY.as_bytes())),
    })
}

fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn parse(text: &str) -> Result<Registry, String> {
    if text.len() > 64 * 1024 {
        return Err("approval registry exceeds 64 KiB".into());
    }
    let registry: Registry = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if registry.schema_version != 1
        || registry.version.is_empty()
        || registry.version.len() > 64
        || registry.approvals.len() > 128
    {
        return Err("unsupported or unbounded approval registry".into());
    }
    if !registry.approvals.is_empty()
        && (registry.engine_sha256 != engine_identity()
            || registry.catalog_sha256 != crate::query_expansion::catalog_sha256())
    {
        return Err("query rule approvals need revalidation for this engine/catalog".into());
    }
    let mut requests = HashSet::new();
    let mut ids = HashSet::new();
    for approval in &registry.approvals {
        let proof = &approval.evidence;
        let before: HashSet<_> = proof.baseline_units.iter().collect();
        let after: HashSet<_> = proof.selected_units.iter().collect();
        let rules: HashSet<_> = approval.rule_ids.iter().collect();
        if approval.id.is_empty()
            || approval.id.len() > 64
            || !ids.insert(&approval.id)
            || !digest(&approval.request_sha256)
            || !requests.insert(&approval.request_sha256)
            || approval.rule_ids.is_empty()
            || approval.rule_ids.len() > 32
            || rules.len() != approval.rule_ids.len()
            || !digest(&proof.sha256)
            || !proof.path.starts_with("experiments/")
            || proof.path.len() > 512
            || proof.path.split('/').any(|p| p == "..")
            || before.len() != proof.baseline_units.len()
            || after.len() != proof.selected_units.len()
            || after.len() > 256
            || !before.is_subset(&after)
            || before.len() >= after.len()
            || before
                .iter()
                .chain(&after)
                .any(|id| id.is_empty() || id.len() > 128)
        {
            return Err("invalid, duplicate or non-improving query rule approval".into());
        }
        crate::query_expansion::expand_only("approval validation", &approval.rule_ids)?;
    }
    Ok(registry)
}

pub fn approved_rule_ids(request: &SelectionRequest) -> Result<Vec<String>, String> {
    static APPROVALS: OnceLock<Result<Registry, String>> = OnceLock::new();
    let registry = APPROVALS
        .get_or_init(|| parse(REGISTRY))
        .as_ref()
        .map_err(Clone::clone)?;
    let fingerprint = request_fingerprint(request)?;
    Ok(registry
        .approvals
        .iter()
        .find(|a| a.request_sha256 == fingerprint)
        .map(|a| a.rule_ids.clone())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn registry() -> serde_json::Value {
        serde_json::json!({"schema_version":1,"version":"test",
            "engine_sha256":engine_identity(),"catalog_sha256":crate::query_expansion::catalog_sha256(),
            "approvals":[{"id":"redis","request_sha256":"a".repeat(64),"rule_ids":["nonpositive"],
                "evidence":{"path":"experiments/proof.json","sha256":"b".repeat(64),
                    "baseline_units":["updates"],"selected_units":["updates","nonpositive"]}}]})
    }
    #[test]
    fn approval_needs_gain_without_loss_and_current_engine() {
        assert!(parse(&registry().to_string()).is_ok());
        for field in ["engine_sha256", "catalog_sha256"] {
            let mut value = registry();
            value[field] = "c".repeat(64).into();
            assert!(parse(&value.to_string()).is_err());
        }
        for after in [
            serde_json::json!(["updates"]),
            serde_json::json!(["nonpositive", "other"]),
        ] {
            let mut value = registry();
            value["approvals"][0]["evidence"]["selected_units"] = after;
            assert!(parse(&value.to_string()).is_err());
        }
        let mut value = registry();
        value["approvals"][0]["rule_ids"] = serde_json::json!(["unknown"]);
        assert!(parse(&value.to_string()).is_err());
        let mut value = registry();
        let extra = value["approvals"][0].clone();
        value["approvals"].as_array_mut().unwrap().push(extra);
        assert!(parse(&value.to_string()).is_err());
    }
}
