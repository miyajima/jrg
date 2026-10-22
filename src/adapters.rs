//! Adapters for a result-list JSON object and an already scraped Markdown page.
use crate::selection::{Block, Document};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

pub fn search_results(input: &Value) -> Result<Vec<Document>, String> {
    let results = input
        .get("results")
        .and_then(Value::as_array)
        .ok_or("results must be an array")?;
    results
        .iter()
        .enumerate()
        .map(|(i, result)| {
            let field = |key| {
                result
                    .get(key)
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("result needs string {key}"))
            };
            let id = result
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("result-{}", i + 1));
            Ok(Document {
                id,
                url: field("url")?.into(),
                title: field("title")?.into(),
                blocks: vec![Block {
                    id: "snippet".into(),
                    kind: "result".into(),
                    text: field("text")?.into(),
                    parent_id: None,
                    related_ids: Vec::new(),
                    start_line: None,
                    end_line: None,
                }],
            })
        })
        .collect()
}

pub fn markdown_page(input: &Value) -> Result<Vec<Document>, String> {
    let field = |key| {
        input
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("page needs string {key}"))
    };
    let markdown = field("markdown")?;
    let lines: Vec<&str> = markdown.split_inclusive('\n').collect();
    let heading = Regex::new(r"^(#{1,6})[ \t]+").unwrap();
    let note = Regex::new(r"^\[\^([^\]]+)\]:").unwrap();
    let reference = Regex::new(r"\[\^([^\]]+)\]").unwrap();
    let list = Regex::new(r"^\s*(?:[-+*]|[0-9]+[.)])\s").unwrap();
    let mut blocks = Vec::<Block>::new();
    let mut headings = Vec::<(usize, String)>::new();
    let mut notes = HashMap::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        let start = i;
        let line = lines[i].trim_end_matches(['\r', '\n']);
        let id = format!("line-{}", i + 1);
        let mut parent_id = headings.last().map(|(_, id)| id.clone());
        let kind;
        if let Some(m) = heading.captures(line) {
            let level = m[1].len();
            while headings.last().is_some_and(|(l, _)| *l >= level) {
                headings.pop();
            }
            parent_id = headings.last().map(|(_, id)| id.clone());
            headings.push((level, id.clone()));
            kind = "heading";
            i += 1;
        } else if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            kind = "code";
            let marker = line.trim_start().chars().next().unwrap();
            let width = line
                .trim_start()
                .chars()
                .take_while(|c| *c == marker)
                .count();
            i += 1;
            while i < lines.len() {
                let closing = lines[i].trim();
                i += 1;
                if closing.chars().take_while(|c| *c == marker).count() >= width
                    && closing.chars().all(|c| c == marker)
                {
                    break;
                }
            }
        } else if let Some(m) = note.captures(line) {
            kind = "note";
            if notes.insert(m[1].to_owned(), id.clone()).is_some() {
                return Err("duplicate footnote definition".into());
            }
            i += 1;
            while i < lines.len() && (lines[i].starts_with("    ") || lines[i].starts_with('\t')) {
                i += 1;
            }
        } else {
            kind = if line.trim_start().starts_with('|') {
                "table"
            } else if list.is_match(line) {
                "list"
            } else {
                "paragraph"
            };
            i += 1;
            while i < lines.len() && !lines[i].trim().is_empty() {
                let next = lines[i];
                if heading.is_match(next)
                    || note.is_match(next)
                    || next.trim_start().starts_with("```")
                    || next.trim_start().starts_with("~~~")
                    || (kind == "table") != next.trim_start().starts_with('|')
                    || (kind == "paragraph" && list.is_match(next))
                {
                    break;
                }
                i += 1;
            }
        }
        blocks.push(Block {
            id,
            kind: kind.into(),
            text: lines[start..i].concat(),
            parent_id,
            related_ids: Vec::new(),
            start_line: Some(start + 1),
            end_line: Some(i),
        });
    }
    for block in &mut blocks {
        if block.kind == "code" {
            continue;
        }
        for m in reference.captures_iter(&block.text) {
            let id = notes
                .get(&m[1])
                .ok_or_else(|| format!("missing footnote definition: {}", &m[1]))?;
            if id != &block.id && !block.related_ids.contains(id) {
                block.related_ids.push(id.clone());
            }
        }
    }
    Ok(vec![Document {
        id: field("id")?.into(),
        url: field("url")?.into(),
        title: field("title")?.into(),
        blocks,
    }])
}
