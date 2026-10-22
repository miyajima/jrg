# 取得済みのWeb情報の選定

`jrg-select`は、呼び出し側が取得した候補をBM25で順位付けし、必要な原文のまとまりを予算内で返します。検索サービスやスクレイパーへは接続しません。従来の`jrg` CLIとソース取得MCPの動作は変えません。

## CLI

```sh
cargo build --bin jrg-select
target/debug/jrg-select --adapter search-results request.json
target/debug/jrg-select --adapter markdown-page page-request.json
cat document-request.json | target/debug/jrg-select -
```

検索結果一覧用のリクエスト:

```json
{
  "query": "解約条件と違約金",
  "max_tokens": 1000,
  "input": {
    "results": [
      {"id": "r1", "url": "https://example.com/terms", "title": "利用規約", "text": "解約時の違約金は無料です。"}
    ]
  }
}
```

解析済みのMarkdown本文用のリクエスト:

```json
{
  "query": "違約金",
  "max_tokens": 1000,
  "input": {
    "id": "terms",
    "url": "https://example.com/terms",
    "title": "利用規約",
    "markdown": "# 利用規約\n\n## 解約\n\n違約金は無料です。[^a]\n\n[^a]: 法人契約を除く。\n"
  }
}
```

この2つは汎用形式のアダプターです。特定の検索・スクレイピングサービスへの接続や、そのサービス固有の返却形式の自動判定は含みません。サービスの応答をこの形式に変換して渡してください。

## Rust APIと文書形式

`jrg::selection::select(&SelectionRequest)`を呼びます。`jrg::adapters::{search_results, markdown_page}`はアダプター入力から`Vec<Document>`を作ります。

`query_expansions`に、呼び出し側が作った翻訳や言い換えを指定できます。元の`query`を残し、追加表現を改行で結合して順位付けに使います。例えば、日本語の`query`に`"query_expansions": ["cancellation conditions and exceptions"]`を加えると、対応する英語の原文も選びやすくなります。省略または空配列の場合は主質問だけを入力に使います。CLIの3種類のアダプターで共通して使えます。

追加表現は最大4件で、各表現には検索可能な語が必要です。前後の空白を除いた追加表現、区切りの改行、元の`query`を合わせて4,096 UTF-8バイトまでです。元の`query`自体も検索可能な語を含む必要があります。翻訳は呼び出し側が質問から用意します。

標準運用では、呼び出し側AIが原文を見る前に、質問の条件・否定・版名・数値・識別子を保った英語を用意します。意味が明確なら正式なAPI用語と数値境界の短い同義表現も含め、答えや設定値を推測させません。jrg自身は外部モデルを呼ばず、受け取った英語を`query_expansions`として使います。既存クライアントとの互換性のため、英語は任意入力です。

ルールの既定は`verified`です。[承認台帳](../data/verified-query-rules.v1.json)に記録した、質問・英語・予算・解析済み文書の完全な一致条件でのみ、比較検証で改善したルールの組み合わせを適用します。未登録の条件は英語だけで選定します。初期承認は記録済みRedisとS3の各1条件に限定し、サービスや用語全般へ承認を広げません。コード・依存関係・カタログが変わった場合も再検証が必要です。

`--query-rules off`（APIは`QueryRuleMode::Off`）は呼出元の英語を残してルールだけを無効にします。`--query-rules builtin`は全カタログを使う比較実験用で、MCP/RTKは常に`verified`を指定します。単独ルールの比較は`--query-rules builtin --rule-ids nonpositive`です。追加は最大4表現・512バイト、元の入力と合わせて4,096バイト以内です。[ルールと承認の更新手順](../data/QUERY_EXPANSIONS.md)を参照してください。

`jrg-select --query-policy-info`でコード・カタログ・台帳のSHA-256、`--rule-request-fingerprint`で解析済みリクエストのSHA-256を確認できます。これらは監査用で、利用者の入力で承認を追加する機能ではありません。

Rustで`SelectionRequest`を構造体リテラルから作っている呼び出し側は、追加表現を使わない場合も`query_expansions: Vec::new()`を追加してください。JSON入力ではフィールドを省略できます。

CLIの既定アダプター`documents`では、`input`の代わりに`documents`を直接渡します。

```json
{
  "query": "違約金",
  "max_tokens": 1000,
  "documents": [{
    "id": "terms",
    "url": "https://example.com/terms",
    "title": "利用規約",
    "blocks": [
      {"id": "h1", "kind": "heading", "text": "## 解約"},
      {"id": "p1", "kind": "paragraph", "text": "違約金は無料です。", "parent_id": "h1", "related_ids": ["n1"]},
      {"id": "n1", "kind": "note", "text": "法人契約を除く。"}
    ]
  }]
}
```

- `kind`: `heading`, `paragraph`, `list`, `table`, `note`, `result`, `code`。
- 文書IDは入力内で一意、ブロックIDは文書内で一意です。参照切れ、親の循環、空の出典URIはエラーです。`related_ids`間の循環は有限集合として処理します。
- 見出しの祖先と`related_ids`は選定対象と一緒に返します。これらの集合が予算に入らない場合は、集合を途中で切らずに見送ります。
- 表、リスト、コードは一塊のブロックです。MarkdownアダプターはATX見出し、段落、連続リスト、行頭`|`の表、コードフェンス、`[^id]`脚注を扱います。コード内の記号は脚注として解釈しません。
- 自動で認識する注記はMarkdown脚注です。その他の適用条件・例外・注記は、呼び出し側が同じブロックに含めるか`related_ids`で関連付けてください。意味に基づく依存関係を保証する機能ではありません。
- Markdownの文字列は書き換えずに保持します。`start_line`と`end_line`は入力Markdown上の1始まりの行番号で、元HTMLのDOM位置ではありません。

## 結果と予算

結果には`documents`（原文と出典）、`omitted_count`、`duplicate_count`、`status`、`tokens`、`max_tokens`を返します。`source_hash`は選定前の文書構造をJSON化したSHA-256で、ネットワーク取得時の元HTMLのハッシュではありません。

- `selected`: 1つ以上のまとまりを選びました。
- `no_lexical_overlap`: 入力候補に検索語との語彙上の重なりがありません。情報の不存在を意味しません。
- `budget_exhausted`: 関連候補はありますが、文脈を含むまとまりが予算に入りません。

順位付けだけにNFKC・カナ統一を適用し、返す原文には適用しません。語の重なりに基づく検索なので、同義語や翻訳を含む意味検索を保証しません。文書のURL・タイトル・全ブロックが同じ場合だけ重複としてまとめ、別出典や別版は統合しません。

予算は、出典・省略件数なども含む**コンパクトJSON本体全体**を`o200k_base`で計数します。CLI末尾の改行、MCPやHTTPの外側の構造、別モデルの実際の課金トークン数は含みません。上限は64〜26,000トークン、入力JSONは4 MiB、文書256件・ブロック4,096件までです。見出し等を候補ごとに加えた順位付け対象は合計8 MiBまでに制限します。

## MCPとの接続

`integrations/mcp/jrg_select_proxy.py`はローカルのstdio MCPと中継します。検索結果用ツールと本文用ツールを名前で指定し、ツール結果を`jrg-select`に直接渡します。AIが原文を一度受け取ってから転送する形にはしません。

```sh
python3 integrations/mcp/jrg_select_proxy.py \
  --selector "$PWD/target/debug/jrg-select" \
  --search-tool search --page-tool fetch --max-tokens 1000 \
  --upstream python3 tests/fixtures/selection_mock_mcp.py
```

検索は呼出し引数の`query`または`q`を選定に使用し、`jrg_focus`で上書きできます。本文取得は`jrg_focus`で調査目的を明示してください。この追加引数は上流へ転送しません。上流ツールは前述の汎用JSON形式を、構造化データまたはJSONのテキストとして返す必要があります。

両ツールの定義に、任意の`jrg_focus_en`も追加します。日本語の目的で英語資料を調べる場合、呼び出し側AIが返却原文を見る前に英語を用意し、元の条件・否定・版名・数値・識別子を保つよう指示します。意味が明確な場合は正式なAPI用語と数値境界の短い同義表現も使い、答えや事実は追加させません。プロキシはこれを`query_expansions`へ渡し、`jrg_focus`とともに上流の実ツールへの引数から取り除きます。元の日本語の目的は置き換えません。

例えば、本文取得の引数は`{"url":"https://example.test/terms","jrg_focus":"解約条件と例外","jrg_focus_en":"cancellation conditions and exceptions"}`です。`jrg_focus_en`は必須ではなく、翻訳の生成や品質判定をjrgが行うわけではありません。新しいセレクターをビルド後、プロキシを再起動してツール定義を再取得してください。

対象ツールの返却値と出力スキーマは選定結果用に変更します。原文を`structuredContent`やメタデータへ残して返しません。選定・解析に失敗した場合はエラーを返し、巨大な原文へ黙って戻しません。対象外の呼出しは通常の中継です。

初期版はmacOS/Linux上のstdio接続のみです。HTTP/OAuth接続、上流からの対話的コールバック、混在した画像・リソースを含む結果の選定、永続キャッシュと原文再取得ツールは含みません。接続経路を変更できない内蔵ツールには適用できません。

## RTK Proxyとの接続

`integrations/rtk-proxy/`は、RTK Proxy v1.0.1の公開`Transform`と`Pipeline`を使う独立した実行ファイルです。上流のフォークは不要です。ビルド・起動方法は[連携用README](../integrations/rtk-proxy/README.md)を参照してください。

```text
取得ツールの結果を含むリクエスト
  → RTK ProxyのTransform
  → jrg-select（共通APIとアダプター）
  → 選定結果に置き換えたリクエスト
  → モデルAPI
```

対応する経路はAnthropic Messagesの`POST /v1/messages`です。検索・本文取得ツールの結果を、その直前の`tool_use`と関連付け、呼出し引数または当時のユーザー質問から選定目的を決めます。後続の質問によって過去の選定結果が変わることを避けるため、元の呼出しにひも付いた目的と選定結果だけのキャッシュを使います。セレクターや選定設定を変更した場合はプロキシを再起動してください。

対象ツール名は起動時に指定します。結果は前述の汎用JSON形式で渡す必要があり、異なるサービス形式には変換処理が必要です。選定失敗は短いツールエラーに置き換え、元の大量の本文を転送しません。OpenAI ResponsesとChat Completionsの経路には対応していません。

RTKとjrgのSQLite依存関係が異なるため、別のCargoワークスペースから`jrg-select`を子プロセスとして呼びます。jrg本体のMITライセンスとは別に、RTK Proxy依存部分には上流のBUSL-1.1とAdditional Use Grantの条件が適用されます。

## 検証

```sh
cargo test --test selection --test selection_cli
cargo build --bin jrg-select
python3 tests/selection_mcp.py
cargo test
cargo test --manifest-path integrations/rtk-proxy/Cargo.toml --locked
```

固定した原文、模擬MCP、ローカルの模擬HTTP上流と実際のRTKルーターを使う機能テストです。実サービスの認証・接続や回答品質の改善、モデル利用料金の削減を証明するものではありません。

## 実Webデータでの評価

2026-10-22に実検索106候補と公式本文5件で比較しました。同じ言語の本文では指定根拠ブロックの保持が改善した一方、検索結果一覧の指定公式URLの保持は改善せず、日本語の質問と英語本文では悪化しました。小規模な固定データによる評価で、回答精度や実料金の改善を示すものではありません。[比較条件・全結果・再現手順](../experiments/2026-10-22-web-selection-eval/REPORT.md)を参照してください。

その後、資料を渡していないAIが生成した英語を追加し、以前に評価していない公式技術資料8件で比較しました。1,024トークンでは事前指定の根拠保持が6/17から14/17へ改善し、全指定根拠が残る資料は1/8から5/8でした。質問・英語・正解・原本は選定前に固定し、英語とエンジンは結果を見て調整していません。意味上の回答精度や検索結果の順位付けとは別の評価です。[全48条件・失敗例・評価の限界](../experiments/2026-10-22-web-selection-heldout/REPORT.md)を参照してください。

頻出表現ルールと英語生成指示を改善し、さらに別の公式資料8件で比較しました。1,024トークンでは従来の11/17に対して、同じ英語＋ルールは11/17、改善英語＋ルールは13/17でした。2,048では13/17・13/17・16/17です。今回の新資料ではルール単独の上積みは確認できず、生成表現の違いを含む条件で改善しました。[3条件の全48結果・回帰確認・更新手順](../experiments/2026-10-22-web-selection-rules/REPORT.md)を参照してください。

追加で改善英語・ルール無効の16条件を比較したところ、1,024では13/17で併用時と同じ、2,048では17/17で併用時の16/17を上回りました。今回の8資料では、改善英語へルールを常時加える効果は確認できず、Queueで指定根拠を1単位失いました。[全4条件の比較・採用判断](../experiments/2026-10-22-web-selection-rules/ablation-improved-off/REPORT.md)を参照してください。この追加検証では製品の既定値は変更していません。

2026-10-22に改善英語を標準の生成指示とし、ルールの既定を検証済み条件だけに変更しました。新しい8資料の再実行は1,024で13/17、2,048で17/17を維持し、MCP/RTKの実プロセスによる中継も確認しています。[承認の証拠・再現手順・中継検証](../experiments/2026-10-22-verified-selection/REPORT.md)を参照してください。
