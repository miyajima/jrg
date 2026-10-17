//! Syntax scopes from ast-grep's verified tree-sitter parsers.
#[derive(Clone, Debug)]
pub struct Span {
    pub start: usize,
    pub open: usize,
    pub end: usize,
    pub function: bool,
}
pub fn parse(source: &str, extension: &str) -> Option<Vec<Span>> {
    Analysis::get(source, extension).map(|value| value.spans.clone())
}

#[derive(Clone, Debug)]
pub struct Predicate {
    pub rows: std::ops::Range<usize>,
    pub branch: std::ops::Range<usize>,
}
pub struct Analysis {
    pub spans: Vec<Span>,
    pub predicates: Vec<Predicate>,
}
impl Analysis {
    pub fn get(source: &str, extension: &str) -> Option<std::sync::Arc<Self>> {
        CACHE.with(|cache| cache.borrow_mut().get(source, extension))
    }
    fn build(source: &str, extension: &str) -> Option<Self> {
        use ast_grep_language::{LanguageExt, SupportLang};
        let lang = match extension {
            "rs" => SupportLang::Rust,
            "py" => SupportLang::Python,
            "ts" => SupportLang::TypeScript,
            "tsx" => SupportLang::Tsx,
            "js" | "jsx" | "mjs" | "cjs" => SupportLang::JavaScript,
            _ => return None,
        };
        let ast = lang.ast_grep(source);
        let root = ast.root();
        let lines: Vec<_> = source.lines().collect();
        let mut spans = Vec::new();
        let mut predicates = Vec::new();
        for node in root.dfs() {
            let kind = node.kind();
            if kind == "ERROR" || node.is_missing() {
                return None;
            }
            if matches!(
                kind.as_ref(),
                "if_expression"
                    | "if_statement"
                    | "elif_clause"
                    | "while_expression"
                    | "while_statement"
            ) && let Some(condition) = node.field("condition")
            {
                let end = condition.end_pos();
                let branch = node.field("consequence").or_else(|| node.field("body"));
                let branch = branch.as_ref().unwrap_or(&condition);
                let branch_end = branch.end_pos();
                predicates.push(Predicate {
                    rows: condition.start_pos().line()
                        ..end.line() + usize::from(end.column(&condition) > 0),
                    branch: node.start_pos().line()
                        ..branch_end.line() + usize::from(branch_end.column(branch) > 0),
                });
            }
            let function = matches!(
                kind.as_ref(),
                "function_item"
                    | "function_definition"
                    | "function_declaration"
                    | "generator_function_declaration"
                    | "method_definition"
                    | "arrow_function"
                    | "function_expression"
            );
            let branch = matches!(
                kind.as_ref(),
                "if_expression"
                    | "match_expression"
                    | "match_arm"
                    | "if_statement"
                    | "elif_clause"
                    | "else_clause"
                    | "for_statement"
                    | "while_statement"
                    | "try_statement"
                    | "switch_statement"
                    | "switch_case"
                    | "catch_clause"
            );
            if !function && !branch {
                continue;
            }
            let mut first = node.start_pos().line();
            let end_pos = node.end_pos();
            let end = end_pos.line() + usize::from(end_pos.column(&node) > 0);
            let open = node.field("body").map_or(first, |n| {
                if extension == "py" {
                    n.start_pos().line().saturating_sub(1).max(first)
                } else {
                    n.start_pos().line()
                }
            });
            if function {
                while first > 0
                    && (lines[first - 1].trim_start().starts_with("#[")
                        || lines[first - 1].trim_start().starts_with("///")
                        || lines[first - 1].trim_start().starts_with('@'))
                {
                    first -= 1;
                }
                // Arrow functions belong to their complete declaration line.
                if let Some(parent) = node.parent()
                    && parent.kind() == "decorated_definition"
                {
                    first = parent.start_pos().line();
                }
            }
            spans.push(Span {
                start: first,
                open,
                end,
                function,
            });
        }
        Some(Self { spans, predicates })
    }
}
thread_local! {
    static CACHE: std::cell::RefCell<Cache> = std::cell::RefCell::new(Cache::new(16*1024*1024));
}
struct Entry {
    source: String,
    extension: String,
    analysis: Option<std::sync::Arc<Analysis>>,
    bytes: usize,
}
struct Cache {
    entries: std::collections::VecDeque<Entry>,
    limit: usize,
    bytes: usize,
    #[cfg(test)]
    builds: usize,
}
impl Cache {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            bytes: 0,
            entries: Default::default(),
            #[cfg(test)]
            builds: 0,
        }
    }
    fn get(&mut self, source: &str, extension: &str) -> Option<std::sync::Arc<Analysis>> {
        if let Some(index) = self
            .entries
            .iter()
            .position(|e| e.extension == extension && e.source == source)
        {
            let entry = self.entries.remove(index).unwrap();
            let result = entry.analysis.clone();
            self.entries.push_back(entry);
            return result;
        }
        #[cfg(test)]
        {
            self.builds += 1;
        }
        let analysis = Analysis::build(source, extension).map(std::sync::Arc::new);
        let bytes = source
            .len()
            .saturating_add(extension.len())
            .saturating_add(std::mem::size_of::<Entry>())
            .saturating_add(analysis.as_ref().map_or(0, |a| {
                std::mem::size_of::<Analysis>()
                    + a.spans.capacity() * std::mem::size_of::<Span>()
                    + a.predicates.capacity() * std::mem::size_of::<Predicate>()
            }));
        if bytes <= self.limit {
            while self.entries.len() >= 32 || self.bytes.saturating_add(bytes) > self.limit {
                let evicted = self.entries.pop_front().unwrap();
                self.bytes -= evicted.bytes;
            }
            self.entries.push_back(Entry {
                source: source.into(),
                extension: extension.into(),
                analysis: analysis.clone(),
                bytes,
            });
            self.bytes += bytes;
        }
        analysis
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_reuses_analysis_but_changes_with_content_language_and_parse_validity() {
        let mut cache = Cache::new(4096);
        let source = "fn f(a: bool, b: bool) {\n if a\n    || b\n { return; }\n}\n";
        let first = cache.get(source, "rs").unwrap();
        let again = cache.get(source, "rs").unwrap();
        assert_eq!(first.predicates[0].rows, 1..3);
        assert_eq!(again.spans[0].end, first.spans[0].end);
        assert_eq!(
            cache.builds, 1,
            "the same source should not be parsed twice"
        );
        assert!(cache.get(source, "py").is_none());
        assert!(cache.get("fn broken( {", "rs").is_none());
        assert!(cache.get("fn broken( {", "rs").is_none());
        assert_eq!(
            cache.builds, 3,
            "invalid syntax should also reuse its result"
        );
        let changed = cache.get("fn f() {}\n", "rs").unwrap();
        assert!(changed.predicates.is_empty());
        assert_eq!(changed.spans[0].end, 1);
        assert_eq!(
            first.predicates[0].rows,
            1..3,
            "old results must stay immutable"
        );
    }
    #[test]
    fn cache_eviction_and_oversized_sources_preserve_correct_analysis() {
        let mut cache = Cache::new(1024);
        let old = cache.get("fn old() {}", "rs").unwrap();
        for n in 0..40 {
            let source = format!("fn f{n}() {{\n if true {{ return; }}\n}}\n");
            let result = cache.get(&source, "rs").unwrap();
            assert_eq!(result.predicates[0].rows, 1..2);
            assert!(cache.bytes <= 1024);
        }
        assert_eq!(old.spans[0].end, 1);
        let large = format!("fn large() {{\n{}\n}}", "// comment\n".repeat(200));
        let result = cache.get(&large, "rs").unwrap();
        assert_eq!(result.spans[0].end, large.lines().count());
        let builds = cache.builds;
        assert!(cache.get(&large, "rs").is_some());
        assert_eq!(
            cache.builds,
            builds + 1,
            "oversized entries must not be retained"
        );
        assert!(cache.bytes <= 1024);
    }
    #[test]
    fn handles_templates_object_parameters_decorators_and_raw_strings() {
        for (ext, source, marker) in [
            (
                "ts",
                "export const f = ({x}: {x: number}) => {\n const t = `fake } ${x}`;\n if (x) {\n  target();\n } else {\n  fallback();\n }\n};\n",
                "target",
            ),
            (
                "js",
                "function f() {\n const t = /[{}]/g;\n target();\n}\n",
                "target",
            ),
            (
                "py",
                "@decorate\ndef f(\n    value,\n):\n    if value:\n        target()\n    else:\n        fallback()\n\ndef later():\n    pass\n",
                "target",
            ),
            (
                "rs",
                "#[test]\nfn f() {\n let s = r###\" } { \"###;\n if true {\n  target();\n } else {\n  fallback();\n }\n}\n",
                "target",
            ),
        ] {
            let spans = parse(source, ext).expect(ext);
            let hit = source.lines().position(|s| s.contains(marker)).unwrap();
            let f = spans
                .iter()
                .find(|s| s.function && s.start == 0 && s.start <= hit && hit < s.end)
                .expect(ext);
            assert!(source.lines().take(f.end).any(|s| s.contains(marker)));
            if ext != "js" {
                assert!(source.lines().take(f.end).any(|s| s.contains("fallback")));
            }
            assert!(f.open < hit);
        }
    }
    #[test]
    fn unsupported_or_invalid_syntax_requests_explicit_fallback() {
        assert!(parse("fn broken( {", "rs").is_none());
        assert!(parse("echo hi", "sh").is_none());
    }
}
