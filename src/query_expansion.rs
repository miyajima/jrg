//! Bounded, source-independent query aliases from a versioned built-in catalog.
use serde::Deserialize;
use std::{collections::HashSet, sync::OnceLock};
use unicode_normalization::UnicodeNormalization;

const CATALOG: &str = include_str!("../data/query-expansions.v1.json");
const MAX_QUERY_BYTES: usize = 4096;
const MAX_ADDED_BYTES: usize = 512;
const MAX_ADDED_PHRASES: usize = 4;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleSet {
    schema_version: u32,
    version: String,
    rules: Vec<Rule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    id: String,
    phrases: Vec<String>,
    #[serde(default)]
    require_any: Vec<String>,
    aliases: Vec<String>,
}

fn words(text: &str) -> Vec<String> {
    text.nfkc()
        .collect::<String>()
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn phrase_matches(segments: &[Vec<String>], phrase: &str) -> bool {
    let phrase = words(phrase);
    segments.iter().any(|segment| {
        segment
            .windows(phrase.len())
            .enumerate()
            .any(|(i, window)| {
                window == phrase
                    && (i == 0 || !matches!(segment[i - 1].as_str(), "not" | "never" | "without"))
            })
    })
}

fn parse_rules(text: &str) -> Result<RuleSet, String> {
    if text.len() > 32 * 1024 {
        return Err("query expansion catalog exceeds 32 KiB".into());
    }
    let rules: RuleSet = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if rules.schema_version != 1
        || rules.version.is_empty()
        || rules.version.len() > 64
        || rules.rules.len() > 32
    {
        return Err("unsupported or unbounded query expansion catalog".into());
    }
    let mut ids = HashSet::new();
    for rule in &rules.rules {
        if rule.id.is_empty()
            || rule.id.len() > 64
            || !ids.insert(&rule.id)
            || rule.phrases.is_empty()
            || rule.phrases.len() > 16
            || rule.aliases.is_empty()
            || rule.aliases.len() > 8
            || rule.require_any.len() > 8
            || rule
                .phrases
                .iter()
                .chain(&rule.aliases)
                .chain(&rule.require_any)
                .any(|s| s.len() > 128 || words(s).is_empty())
        {
            return Err("invalid or duplicate query expansion rule".into());
        }
    }
    Ok(rules)
}

#[derive(Debug, PartialEq)]
pub struct QueryExpansion {
    pub query: String,
    pub rule_version: String,
    pub applied_rule_ids: Vec<String>,
    pub added_phrases: Vec<String>,
}

/// Expand only the explicit subset; used for controlled approval comparisons.
pub fn expand_only(query: &str, rule_ids: &[String]) -> Result<QueryExpansion, String> {
    expand_selected(query, Some(rule_ids))
}

/// Match only the original question/paraphrases. Added aliases never trigger rules.
/// This function has no source-document argument and never changes the input text.
pub fn expand(query: &str) -> Result<QueryExpansion, String> {
    expand_selected(query, None)
}

fn expand_selected(query: &str, selected: Option<&[String]>) -> Result<QueryExpansion, String> {
    if query.len() > MAX_QUERY_BYTES {
        return Err("query exceeds 4096 bytes".into());
    }
    static RULES: OnceLock<Result<RuleSet, String>> = OnceLock::new();
    let rules = RULES
        .get_or_init(|| parse_rules(CATALOG))
        .as_ref()
        .map_err(Clone::clone)?;
    if let Some(ids) = selected
        && (ids.len() > 32
            || ids
                .iter()
                .any(|id| !rules.rules.iter().any(|r| &r.id == id)))
    {
        return Err("unknown or unbounded query rule subset".into());
    }
    let segments = query.lines().map(words).collect::<Vec<_>>();
    let mut result = QueryExpansion {
        query: query.to_owned(),
        rule_version: rules.version.clone(),
        applied_rule_ids: Vec::new(),
        added_phrases: Vec::new(),
    };
    let mut known = crate::rank::tokens(query)
        .into_iter()
        .collect::<HashSet<_>>();
    for rule in &rules.rules {
        if selected.is_some_and(|ids| !ids.contains(&rule.id)) {
            continue;
        }
        if !rule
            .phrases
            .iter()
            .any(|phrase| phrase_matches(&segments, phrase))
            || (!rule.require_any.is_empty()
                && !rule
                    .require_any
                    .iter()
                    .any(|phrase| phrase_matches(&segments, phrase)))
        {
            continue;
        }
        let before = result.added_phrases.len();
        for alias in &rule.aliases {
            let tokens = crate::rank::tokens(alias);
            let extra = alias.len() + 1;
            if result.added_phrases.len() >= MAX_ADDED_PHRASES
                || result.query.len() + extra > MAX_QUERY_BYTES
                || result.query.len() - query.len() + extra > MAX_ADDED_BYTES
                || tokens.iter().all(|t| known.contains(t))
            {
                continue;
            }
            result.query.push('\n');
            result.query.push_str(alias);
            result.added_phrases.push(alias.clone());
            known.extend(tokens);
        }
        if result.added_phrases.len() > before {
            result.applied_rule_ids.push(rule.id.clone());
        }
    }
    Ok(result)
}

/// Fingerprint of the exact alias catalog used by this build.
pub fn catalog_sha256() -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(CATALOG.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_rejects_invalid_schema_empty_patterns_and_duplicate_ids() {
        for json in [
            r#"{"schema_version":2,"version":"x","rules":[]}"#,
            r#"{"schema_version":1,"version":"","rules":[]}"#,
            r#"{"schema_version":1,"version":"x","rules":[{"id":"a","phrases":[" "],"aliases":["b"]}]}"#,
            r#"{"schema_version":1,"version":"x","rules":[{"id":"a","phrases":["x"],"aliases":["b"]},{"id":"a","phrases":["z"],"aliases":["c"]}]}"#,
        ] {
            assert!(parse_rules(json).is_err());
        }
    }
}
