use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Block {
    pub id: String,
    pub kind: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub id: String,
    pub url: String,
    pub title: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionRequest {
    pub query: String,
    /// Caller-provided translations or paraphrases; never generated from source text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_expansions: Vec<String>,
    pub max_tokens: usize,
    pub documents: Vec<Document>,
}

fn expanded_query(request: &SelectionRequest) -> Result<String, String> {
    if request.query.len() > 4096 || crate::rank::tokens(&request.query).is_empty() {
        return Err("query must contain searchable terms and be at most 4096 bytes".into());
    }
    if request.query_expansions.len() > 4 {
        return Err("query_expansions must contain at most 4 strings".into());
    }
    let mut query = request.query.clone();
    for expansion in &request.query_expansions {
        let expansion = expansion.trim();
        if query.len() + 1 + expansion.len() > 4096 {
            return Err("combined query and expansions must be at most 4096 bytes".into());
        }
        if expansion.is_empty() || crate::rank::tokens(expansion).is_empty() {
            return Err("query_expansions must contain nonempty searchable strings".into());
        }
        query.push('\n');
        query.push_str(expansion);
    }
    Ok(query)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectedDocument {
    #[serde(flatten)]
    pub document: Document,
    pub source_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectionResponse {
    pub documents: Vec<SelectedDocument>,
    pub omitted_count: usize,
    pub duplicate_count: usize,
    pub status: String,
    pub tokens: usize,
    pub max_tokens: usize,
}

/// Rank all supplied candidates lexically, then pack complete context groups.
/// No URL is fetched. Positions refer to the caller's extracted text, not HTML.
pub fn select(request: &SelectionRequest) -> Result<SelectionResponse, String> {
    select_with_query_rules(request, QueryRuleMode::Verified)
}

#[derive(Debug, Clone, Copy)]
pub enum QueryRuleMode {
    Verified,
    Builtin,
    Off,
}

/// Verified is the safe default. Builtin is an experimental all-rules comparison.
/// Caller-provided English is retained and validated identically in every mode.
pub fn select_with_query_rules(
    request: &SelectionRequest,
    mode: QueryRuleMode,
) -> Result<SelectionResponse, String> {
    select_impl(request, mode, None)
}

/// Explicit subsets are for offline approval experiments, never proxy metadata.
pub fn select_with_rule_ids(
    request: &SelectionRequest,
    rule_ids: &[String],
) -> Result<SelectionResponse, String> {
    select_impl(request, QueryRuleMode::Builtin, Some(rule_ids))
}

fn select_impl(
    request: &SelectionRequest,
    mode: QueryRuleMode,
    rule_ids: Option<&[String]>,
) -> Result<SelectionResponse, String> {
    let query = expanded_query(request)?;
    if !(64..=26_000).contains(&request.max_tokens) {
        return Err("max_tokens must be between 64 and 26000".into());
    }
    if request.documents.len() > 256
        || request
            .documents
            .iter()
            .map(|d| d.blocks.len())
            .sum::<usize>()
            > 4096
        || serde_json::to_vec(request)
            .map_err(|e| e.to_string())?
            .len()
            > 4 * 1024 * 1024
    {
        return Err("candidate limit exceeded (256 documents, 4096 blocks, 4 MiB)".into());
    }
    let query = match mode {
        QueryRuleMode::Verified => {
            crate::query_expansion::expand_only(
                &query,
                &crate::verified_query_rules::approved_rule_ids(request)?,
            )?
            .query
        }
        QueryRuleMode::Builtin => match rule_ids {
            Some(ids) => crate::query_expansion::expand_only(&query, ids)?.query,
            None => crate::query_expansion::expand(&query)?.query,
        },
        QueryRuleMode::Off => query,
    };
    let query = crate::rank::tokens(&query);
    let mut document_ids = HashSet::new();
    let mut unique = Vec::new();
    let mut fingerprints = HashSet::new();
    let mut duplicate_count = 0;
    for document in &request.documents {
        if document.url.trim().is_empty() {
            return Err("documents need a nonempty source URL or URI".into());
        }
        if document.id.is_empty() || !document_ids.insert(&document.id) {
            return Err("document IDs must be nonempty and unique".into());
        }
        let index: HashMap<&str, usize> = document
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id.as_str(), i))
            .collect();
        if index.len() != document.blocks.len() || index.contains_key("") {
            return Err("block IDs must be nonempty and unique within a document".into());
        }
        for block in &document.blocks {
            if block.text.is_empty()
                || !matches!(
                    block.kind.as_str(),
                    "heading" | "paragraph" | "list" | "table" | "note" | "result" | "code"
                )
            {
                return Err("blocks need nonempty text and a supported kind".into());
            }
            if block.start_line.is_some() != block.end_line.is_some()
                || block
                    .start_line
                    .zip(block.end_line)
                    .is_some_and(|(a, b)| a == 0 || a > b)
            {
                return Err("invalid source line range".into());
            }
            for id in block.parent_id.iter().chain(&block.related_ids) {
                if !index.contains_key(id.as_str()) {
                    return Err(format!("unknown context block: {id}"));
                }
            }
            let mut ancestors = HashSet::new();
            let mut cursor = Some(block.id.as_str());
            while let Some(id) = cursor {
                if !ancestors.insert(id) {
                    return Err("cyclic parent relationship".into());
                }
                cursor = document.blocks[index[id]].parent_id.as_deref();
            }
        }
        // Deduplicate only identical documents at the same source URL. Never
        // conflate a mirror or a changed version with an independently cited source.
        let fingerprint =
            serde_json::to_string(&(&document.url, &document.title, &document.blocks))
                .map_err(|e| e.to_string())?;
        if fingerprints.insert(fingerprint) {
            unique.push((document, index));
        } else {
            duplicate_count += document.blocks.len();
        }
    }
    let total: usize = unique.iter().map(|(d, _)| d.blocks.len()).sum();
    let mut candidates = Vec::new();
    let mut terms = Vec::new();
    let mut expanded_bytes = 0usize;
    for (di, (doc, index)) in unique.iter().enumerate() {
        for (bi, block) in doc.blocks.iter().enumerate() {
            if block.kind == "heading" {
                continue;
            }
            let mut text = format!("{}\n{}", doc.title, block.text);
            let mut parent = block.parent_id.as_deref();
            while let Some(id) = parent {
                let ancestor = &doc.blocks[index[id]];
                text.push('\n');
                text.push_str(&ancestor.text);
                parent = ancestor.parent_id.as_deref();
            }
            expanded_bytes = expanded_bytes.saturating_add(text.len());
            if expanded_bytes > 8 * 1024 * 1024 {
                return Err("expanded ranking context exceeds 8 MiB".into());
            }
            candidates.push((di, bi));
            terms.push(crate::rank::tokens(&text));
        }
    }
    let scores = crate::rank::bm25_scores(&terms, &query);
    let mut ranked: Vec<_> = candidates
        .into_iter()
        .zip(scores)
        .filter(|(_, s)| *s > 0.0)
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let has_overlap = !ranked.is_empty();
    let mut chosen = vec![BTreeSet::new(); unique.len()];
    let mut order = Vec::new();
    let mut out = SelectionResponse {
        documents: Vec::new(),
        omitted_count: total,
        duplicate_count,
        status: if has_overlap {
            "budget_exhausted"
        } else {
            "no_lexical_overlap"
        }
        .into(),
        tokens: 0,
        max_tokens: request.max_tokens,
    };
    let count = |response: &mut SelectionResponse| -> Result<usize, String> {
        for _ in 0..8 {
            let n = crate::budget::Counter::new(tiktoken_rs::o200k_base_singleton())
                .count(&serde_json::to_string(response).map_err(|e| e.to_string())?);
            if response.tokens == n {
                return Ok(n);
            }
            response.tokens = n;
        }
        Err("serialized token count did not stabilize".into())
    };
    if count(&mut out)? > request.max_tokens {
        return Err("max_tokens cannot fit response metadata".into());
    }
    for ((di, bi), _) in ranked {
        if chosen[di].contains(&bi) {
            continue;
        }
        let (doc, index) = &unique[di];
        let mut group = BTreeSet::new();
        let mut stack = vec![bi];
        while let Some(i) = stack.pop() {
            if !group.insert(i) {
                continue;
            }
            let block = &doc.blocks[i];
            for id in block.parent_id.iter().chain(&block.related_ids) {
                stack.push(index[id.as_str()]);
            }
        }
        let mut proposed = chosen.clone();
        proposed[di].extend(group);
        let mut proposed_order = order.clone();
        if !proposed_order.contains(&di) {
            proposed_order.push(di);
        }
        let mut trial = out.clone();
        trial.documents.clear();
        for &i in &proposed_order {
            let original = unique[i].0;
            let hash = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(original).map_err(|e| e.to_string())?)
            );
            let mut document = original.clone();
            document.blocks = proposed[i]
                .iter()
                .map(|&j| original.blocks[j].clone())
                .collect();
            trial.documents.push(SelectedDocument {
                document,
                source_hash: hash,
            });
        }
        trial.status = "selected".into();
        trial.omitted_count = total - proposed.iter().map(BTreeSet::len).sum::<usize>();
        if count(&mut trial)? <= request.max_tokens {
            chosen = proposed;
            order = proposed_order;
            out = trial;
        }
    }
    Ok(out)
}
