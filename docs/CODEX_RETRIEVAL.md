# Codexからjrgを使う取得手順

検索結果がjrg内の予算に収まっても、内側exec_commandや外側functions.execの上限で切れることがある。外側・内側の両方を設定する。jrgの既定24,000は維持し、不要な範囲を先に絞る。

1回の一括取得を返すfunctions.execの例:

```javascript
// @exec: {"max_output_tokens": 30000}
const result = await tools.exec_command({
  cmd: "jrg --smart --batch request.json",
  max_output_tokens: 26000
});
text(result.output);
```

先頭のpragmaが外側の上限を設定する。`exec_command`の引数だけでは外側の10,000は変わらない。`text(result)`で結果全体をJSON化すると改行がエスケープされ、本文以外の情報も増えるため、`result.output`を返す。終了コードは呼出し側で確認し、非ゼロなら短く表示する。

独立した複数コマンドを同じセルで返す場合は、内側の出力上限の合計を26,000以下にするか、外側を合計＋余裕分へ増やす。上限が十分でも実行環境がさらに制限する場合があるため、切詰め通知を確認する。巨大なファイル一覧は対象ディレクトリ・globで絞り、内容の読取と混ぜない。予算に合わせて原文を黙ってheadで捨てない。

## 調査段階を減らす

- 初回の内容取得で、必要な実装と関連テストを独立操作としてまとめる。既知のファイル・範囲は直接readし、行数確認だけの往復を避ける。
- 広い正規表現はlocateで候補を見つける。explainは関数名・ガード・結果の確認に使う。既定autoが過剰な関数展開を抑制し、全一致点は残す。
- `scope_limited`は局所範囲を選んだ表示。関数全体が必要なら`scope_read`の範囲をreadするか`--explain-scope full`を使う。`pending`とは別である。
- すべての要求項目を裏付けられたら、行番号を付けるだけの読直しや、まとめる前の念押しを追加しない。全件・不存在の確認には完全検索を使う。
- 出力が切れた場合、CLIの取得済み管理をそのままモデル既読と扱わず、必要範囲を`--repeat`で再取得する。

これは呼出し手順であり、外部ツールが実際に適用した上限や、モデル応答回数の削減をCLIだけで保証する機能ではない。実タスクでは切詰め・必要項目・総入力・応答・経過時間を確認する。

## 0.8.0: 構造化したMCP呼出し

`jrg --smart --mcp --root /absolute/project --session /private/tmp/task-session.json`
は、検索と読取だけを提供するstdio MCPサーバーを起動する。追加ランタイム・ネットワーク・シェル実行は不要。セッションの親フォルダは先に作り、会話ごとに新しいファイル名を使う。別バージョンのセッションは引き継がない。

Codexの接続には`mcp_servers.<name>.command`と`args`を使う（[公式設定](https://learn.chatgpt.com/docs/extend/mcp#stdio-servers)）。今回の比較では`codex exec`より後ろの実行ごとの`-c`で接続し、`mcp_servers.jrg.required=true`を指定する。ユーザー全体の設定を変更しない。`codex mcp get`の設定表示だけでなく、app-serverのツール一覧と実際のMCP呼出しで接続を検証した。

ツール`retrieve`へ次のように直接引数を渡す。これは引数の例であり、シェルの文字列や標準入力用JSONを組み立てる手順ではない。

```json
{
  "operations": [
    {"search": {"patterns": ["rg_eligible"], "paths": ["src"], "regex": true, "intent": "explain"}},
    {"read": {"path": "tests/smart.rs", "start": 1, "end": 120}}
  ]
}
```

- `patterns`は既定で日本語正規化を行う固定文字列。`regex: true`は原文正規表現。正規表現のバックスラッシュも引数の文字列として一度だけ渡す。
- `paths`省略時は固定root。`globs`、`case_insensitive`、`context`、`full_scope`を任意指定できる。明示`context`は従来どおり範囲展開より優先する。
- 1呼出しで最大64操作。既定24,000トークン、`max_tokens`は64〜26,000。検索と読取を統合し、取得済みの重複行を抑制する。`operations: []`と`resume: true`で未表示分を取得する。必要な既読範囲だけを再表示する場合は通常操作と`repeat: true`を使う。
- ツールの返却はMCPのcontent.text。functions.exec経由なら先頭30,000のpragmaを引き続き指定し、ツール結果全体をJSON文字列化しない。サーバーの書出し成功は、外側で切れずにモデルへ届いた証明ではない。
- `--session`指定時、MCP応答の書込・flushが成功してから取得済み範囲を保存する。保存するのは原文パス・内容の指紋・範囲で、原文本文は保存しない。
- 指定root外のパスと、外へ出る明示シンボリックリンクを拒否する。パスの付替え競合に対するOSのセキュリティ境界ではない。ホスト側の権限も必要になる。
- ファイル一覧、完全な不存在確認、機械出力は従来のrg経路を使う。MCPは任意CLI引数や任意コマンドを受け付けない。初期版の対応範囲は検索・UTF-8行読取で、すべてのrgオプションを公開していない。

explainのautoでは、同じ検索で定義の宣言部分に一致した場合、その定義は従来の字句範囲上限内で全体を返す。一方、他の一致箇所は呼出し元の宣言と前後3行を返し、大きな呼出し元関数全体へ展開しない。定義と呼出し箇所が別ファイルでも適用する。定義が選択されない条件分岐の検索、明示文脈、`full_scope: true` / `--explain-scope full`は従来の範囲選択を保つ。字句解析による推定であり、識別子の意味解決・依存関係や関連テストの自動収集ではない。実装と関連テストを同じoperationsへ指定することで独立した取得をまとめる。

stdioのメッセージと初期化は[MCP公式仕様](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)に従い、1行1JSON-RPC、stdoutはプロトコル専用、診断はstderrへ出す。

### 比較後に分かった呼出し手順の注意

0.8の実モデル比較では、ツールを探すためだけの応答が4課題に入り、汎用の検索・発見ツールの説明まで返して入力を増やした。接続を事前検証し、サーバー名を`jrg`に固定した場合は、既知の`tools.mcp__jrg__retrieve`を最初の内容取得から使う。全ツール一覧やスキーマを調査本文として表示しない。接続確認が必要なら、モデルを起動する前にapp-serverのMCP一覧で確認する。次はjrgプロジェクト用の例。

```javascript
// @exec: {"max_output_tokens": 30000}
const result = await tools.mcp__jrg__retrieve({
  operations: [
    {search: {patterns: ["rg_eligible"], paths: ["src"], regex: true, intent: "explain"}},
    {read: {path: "tests/smart.rs", start: 1, end: 120}}
  ]
});
for (const item of result.content ?? []) {
  if (item.type === "text") text(item.text);
}
```

存在を確認したパスを使う。0.8では存在しないパスを1操作へ指定すると、その一括要求全体を実行前に拒否する。外部パスや不正引数の拒否とは別に、ファイル不存在を操作単位のエラーにして他の有効操作を返す改善は今後の候補になる。

この注意は今回の比較後に追加した。名前を固定した初回呼出し手順のモデル応答削減率や、上記の操作単位エラーへの変更は再測定していない。今回の計測結果をこれらの改善後の数値として扱わない。
