use clap::{ArgGroup, Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Clone, Copy, ValueEnum)]
pub enum Encoding {
    Utf8,
    #[value(name = "shift-jis", alias = "sjis", alias = "cp932")]
    ShiftJis,
    #[value(name = "euc-jp")]
    EucJp,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Color {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
pub enum Engine {
    Auto,
    Native,
    Rg,
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
pub enum Intent {
    Locate,
    Explain,
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
pub enum ExplainScope {
    Auto,
    Full,
}

#[derive(Clone, Parser)]
#[command(
    version,
    group(ArgGroup::new("bounded_output").args(["ai", "smart"])),
    about = "日本語の表記ゆれに強い再帰テキスト検索",
    after_help = "例: jrg がっこう ./docs\n    jrg --ai --max-tokens 1000 認証 ./src\n    jrg -i ＡＰＩ -g '*.md' .\n    jrg --regex 'TODO|FIXME' src\n    jrg --encoding shift-jis 東京 legacy\n\n既定は固定文字列検索 + NFKC + カナ統一。--regex は原文に対する正規表現検索。"
)]
pub struct Args {
    /// Read-only local stdio MCP server (requires --root)
    #[arg(long, requires = "root", conflicts_with_all = ["batch", "resume", "pattern", "patterns", "paths"])]
    pub mcp: bool,
    /// Fixed search root for the MCP server
    #[arg(long, requires = "mcp")]
    pub root: Option<PathBuf>,
    /// 検索エンジン（auto は --smart の原文検索で rg、正規化が必要なら native）
    #[arg(long, value_enum, default_value = "auto")]
    pub engine: Engine,
    /// 調査目的。smart explain は文脈未指定時に関数・分岐のまとまりを取得
    #[arg(long, value_enum, default_value = "locate")]
    pub intent: Intent,
    /// explain の過剰な関数展開を自動抑制（full は従来の範囲選択）
    #[arg(long, value_enum, default_value = "auto", requires = "smart")]
    pub explain_scope: ExplainScope,
    /// 自動エンジン選択と、短い完全出力 / 予算内の圧縮出力の選択
    #[arg(long, conflicts_with_all = ["json", "count", "files_with_matches", "quiet", "only_matching", "no_line_number", "column"])]
    pub smart: bool,
    /// 文字列検索の候補をこのクエリとのBM25関連度で並べる（意味検索ではない）
    #[arg(long, requires = "smart", conflicts_with = "mcp")]
    pub rank_query: Option<String>,
    /// 選択したエンジンと文脈量を stderr に表示
    #[arg(long)]
    pub explain_plan: bool,
    /// JSON の operations に指定した検索・行範囲読取を一括実行（- は標準入力）
    #[arg(long, requires = "smart", conflicts_with_all = ["pattern", "patterns"])]
    pub batch: Option<PathBuf>,
    /// この会話で返した原文範囲と未表示範囲を共有するファイル
    #[arg(long, requires = "smart")]
    pub session: Option<PathBuf>,
    /// セッションの未表示範囲だけを再取得する
    #[arg(long, requires = "session", conflicts_with_all = ["pattern", "patterns", "batch", "paths"])]
    pub resume: bool,
    /// 取得済みでも今回要求した範囲を再表示する
    #[arg(long, requires = "session", conflicts_with = "resume")]
    pub repeat: bool,
    #[arg(skip)]
    pub block_context: bool,
    /// 検索する固定文字列（--regex 使用時は正規表現）
    #[arg(required_unless_present_any = ["patterns", "batch", "resume", "mcp"])]
    pub pattern: Option<String>,
    /// 検索パス。省略時は標準入力がパイプなら標準入力、それ以外はカレントディレクトリ
    pub paths: Vec<PathBuf>,
    /// 検索パターンを追加（OR 検索）。-e 使用時は位置引数をパスとして扱う
    #[arg(short = 'e', long = "regexp", value_name = "PATTERN", action = clap::ArgAction::Append)]
    pub patterns: Vec<String>,
    /// 原文を正規表現で検索（表記の正規化は行わない）
    #[arg(long, conflicts_with = "fixed_strings")]
    pub regex: bool,
    /// 固定文字列検索（既定）
    #[arg(short = 'F', long)]
    pub fixed_strings: bool,
    /// NFKC とカナ統一を両方無効にする
    #[arg(long)]
    pub no_normalize: bool,
    /// ひらがな・カタカナの統一を無効にする（NFKC は維持）
    #[arg(long)]
    pub no_kana: bool,
    /// 英字などの大文字小文字を区別しない
    #[arg(short = 'i', long, conflicts_with = "smart_case")]
    pub ignore_case: bool,
    /// 正規化後のパターンに大文字がなければ大文字小文字を区別しない
    #[arg(short = 'S', long)]
    pub smart_case: bool,
    /// 行番号を表示（既定）
    #[arg(short = 'n', long, conflicts_with = "no_line_number")]
    pub line_number: bool,
    /// 行番号を表示しない
    #[arg(long)]
    pub no_line_number: bool,
    /// 一致の開始列を表示（元の行の Unicode 文字数、1 始まり）
    #[arg(long)]
    pub column: bool,
    /// 一致するファイル名だけ表示
    #[arg(short = 'l', long, conflicts_with_all = ["count", "only_matching", "json"])]
    pub files_with_matches: bool,
    /// ファイルごとの一致行数を表示
    #[arg(short = 'c', long, conflicts_with_all = ["only_matching", "json"])]
    pub count: bool,
    /// 一致の有無だけを終了コードで返す
    #[arg(short = 'q', long, conflicts_with_all = ["json", "count", "files_with_matches", "only_matching"])]
    pub quiet: bool,
    /// 一致部分だけを元の表記で表示
    #[arg(short = 'o', long, conflicts_with = "invert_match")]
    pub only_matching: bool,
    /// 一致しない行を選択
    #[arg(short = 'v', long)]
    pub invert_match: bool,
    /// 一致行の前後を表示
    #[arg(short = 'C', long, default_value_t = 0)]
    pub context: usize,
    /// 一致行の前を表示
    #[arg(short = 'B', long, default_value_t = 0)]
    pub before_context: usize,
    /// 一致行の後を表示
    #[arg(short = 'A', long, default_value_t = 0)]
    pub after_context: usize,
    /// ファイルごとの一致行数の上限
    #[arg(short = 'm', long, value_parser = clap::value_parser!(u64).range(1..))]
    pub max_count: Option<u64>,
    /// ファイル名の glob。! で除外（複数指定可）
    #[arg(short = 'g', long = "glob", action = clap::ArgAction::Append)]
    pub globs: Vec<String>,
    /// 隠しファイルも検索
    #[arg(long)]
    pub hidden: bool,
    /// .gitignore / .ignore などの除外規則を無効にする
    #[arg(long)]
    pub no_ignore: bool,
    /// バイナリ判定を無効にする（文字コードの検証は維持）
    #[arg(short = 'a', long)]
    pub text: bool,
    /// 入力文字コード（自動推定は行わない）
    #[arg(long, value_enum, default_value = "utf8")]
    pub encoding: Encoding,
    /// 並列検索数（0 = 自動）
    #[arg(short = 'j', long, default_value_t = 0)]
    pub threads: usize,
    /// 1 行 1 JSON オブジェクトとして表示
    #[arg(long, conflicts_with = "only_matching")]
    pub json: bool,
    /// AI 向け圧縮出力（パス共有・重複排除・一致周辺の抜粋）
    #[arg(long, conflicts_with_all = ["json", "count", "files_with_matches", "quiet", "only_matching", "no_line_number", "column"])]
    pub ai: bool,
    /// AI 出力の総トークン上限（o200k_base、要約行も含む）
    #[arg(long, requires = "bounded_output", default_value_t = 2000, value_parser = clap::value_parser!(u32).range(64..=1_000_000))]
    pub max_tokens: u32,
    /// AI 出力の各抜粋の最大書記素数
    #[arg(long, requires = "bounded_output", default_value_t = 160, value_parser = clap::value_parser!(u32).range(16..=4096))]
    pub snippet_chars: u32,
    /// 一致部分の色付け
    #[arg(long, value_enum, default_value = "auto")]
    pub color: Color,
}

impl Args {
    pub fn prepared(matches: &clap::ArgMatches) -> Self {
        use clap::{FromArgMatches, parser::ValueSource};
        let mut args = Self::from_arg_matches(matches).unwrap();
        let explicit_context = ["context", "before_context", "after_context"]
            .iter()
            .any(|name| matches.value_source(name) == Some(ValueSource::CommandLine));
        if args.intent == Intent::Explain && !explicit_context {
            args.context = 3;
            args.block_context = args.smart;
        }
        if args.smart && matches.value_source("snippet_chars") != Some(ValueSource::CommandLine) {
            args.snippet_chars = 320;
        }
        if args.smart && matches.value_source("max_tokens") != Some(ValueSource::CommandLine) {
            if args.batch.is_some() || args.resume {
                args.max_tokens = 24000;
            } else if args.block_context {
                args.max_tokens = 16000;
            } else {
                args.max_tokens = 8000;
            }
        }
        args
    }

    pub fn inputs(&self, stdin_is_terminal: bool) -> (Vec<String>, Vec<PathBuf>) {
        let mut patterns = self.patterns.clone();
        let mut paths = self.paths.clone();
        if patterns.is_empty() {
            patterns.push(self.pattern.clone().unwrap());
        } else if let Some(path) = &self.pattern {
            paths.insert(0, path.into());
        }
        if paths.is_empty() {
            paths.push(if stdin_is_terminal { "." } else { "-" }.into());
        }
        (patterns, paths)
    }
}
