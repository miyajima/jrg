# 検索履歴とフィードバック

smart/MCP検索は、検索条件・返却範囲・内容のSHA-256・所要時間をローカルのSQLiteへ自動記録します。原文やEmbeddingのベクトルは履歴には保存しません。検索語、パス、要求項目の説明は保存されるため、機密語を残したくない検索では無効にしてください。

```sh
jrg --smart --intent review 'cancel audio' ./src
jrg --history list
jrg --history show REQUEST_ID
jrg --history feedback REQUEST_ID RESULT_ID helpful
jrg --history feedback REQUEST_ID RESULT_ID not_helpful
jrg --history feedback REQUEST_ID RESULT_ID unknown
jrg --history status
jrg --history export > history.jsonl
jrg --history clear
```

`--history`は最初の引数として指定します。通常の`jrg history ./src`は検索のままです。管理コマンドは現在のリポジトリだけが対象です。別の場所を指定する場合は`--history list --root /absolute/repository`を使います。

## 反映の範囲

- `--history-mode auto`（既定）：記録し、同条件の検索の候補順位に評価を反映します。
- `--history-mode record`：記録だけを行います。比較検証に使えます。
- `--history-mode off`または`JRG_HISTORY=off`：記録と反映を止めます。環境変数の無効化指定が優先されます。
- 通常検索、`--ai`、`--json`は履歴を使いません。batchの履歴指定は外側のコマンドに付けます。

検索語、検索対象、要求項目、検索オプション、予算、構文解析・Embeddingの条件が一致した履歴だけを使います。結果そのものを再利用せず、毎回現在のファイルを検索するため、新しいファイルも候補になります。明示したread、単一ファイルのlocate、explain、条件式の優先順位は維持します。

評価は既存候補のスコアにだけ掛け、候補を追加・削除しません。人の評価は最大±10%、エージェントの評価は最大±2.5%です。同じコンテキストのエージェント評価は最新の1件にまとめ、その平均を使います。有効な人の評価があれば最新のものを優先します。`unknown`は同じ出所・コンテキストの評価を撤回します。

CLIの評価は既定で`human`です。エージェントからCLIを呼ぶ場合は`--source agent --context-id TASK_ID`を明示してください。再試行は同じ`--event-id EVENT_ID`を使うと重複しません。同じIDで評価内容を変更するとエラーになります。出所は呼出し側による申告です。

## worktreeと編集

同じGit共通ディレクトリに属するworktreeは履歴を共有します。初回のリポジトリ全体のインデックス構築やwatcherはありません。検索時に必要なファイルの現在の原文を照合します。

ast-grepで解析できる関数は、原文と囲む宣言・条件の指紋が一致し、対応する関数が現在の同じ相対パスに1つだけある場合に評価を引き継ぎます。別の箇所の編集による行番号のずれに追従します。関数本体や囲む条件が変わった場合、候補が重複した場合、削除・改名された場合は反映しません。構文解析できない範囲はファイル全体の指紋一致が必要です。過去の記録の行番号は書き換えません。

別リポジトリ、外部パス、入れ子の別checkoutの原文は、その履歴の評価対象にしません。既存の構文解析キャッシュ・Embeddingキャッシュ・会話の既読管理`--session`はそれぞれ独立しています。

## 返却の記録とMCP

1回の検索/batchにつき`request_id`を発行し、実際に返した完全な原文行を関数・範囲ごとの`result_id`へまとめます。部分返却は`partial`と`ranges`で区別します。要求項目の`id`は検索結果IDとは別です。要求項目のリンクは表示された原文での文字列一致であり、意味上の検証結果ではありません。操作とのリンクもファイル単位の出所です。

履歴の`start`、`end`、`ranges`は0始まりで終端を含みません。本文の行番号は従来どおり1始まりです。`file_operation_ids`は1始まりの操作番号です。

stdoutへの書込みとflushが成功した後に`emitted`となります。これは呼出し側が内容を読んだことや、その結果が有用だったことの証明にはなりません。途中で書込みが失敗した`prepared`の記録には評価できません。

MCPの`retrieve`は任意の`history_mode`と`context_id`を受け付けます。予算に収まる場合、`structuredContent.history`にリクエストID、結果ID、パス、返却行範囲を付けます。本文と履歴メタデータの合計で既存予算を守り、余裕がなければ追加メタデータを省きます。記録は`history`ツールまたはCLIから参照できます。

```json
{"name":"feedback","arguments":{"request_id":"REQUEST_ID","result_id":"RESULT_ID","label":"helpful","event_id":"EVENT_ID","context_id":"TASK_ID"}}
```

MCPの`feedback`は常に`agent`として記録します。同じタスクの複数呼出しには安定した`context_id`を渡してください。検索時の省略は接続単位、評価時の省略はその検索のコンテキストを引き継ぎます。`structuredContent`の`path`は現在のrootからのパス、`repo_path`と保存した履歴のパスはリポジトリ基準です。`history`ツールは`list`、`show`、`status`を提供し、設定root内の記録だけを返します。list/statusは最近100件を走査した範囲です。MCP経由の削除・exportはありません。

## 保存先と障害時

保存先は`$XDG_STATE_HOME/jrg/history-v1.sqlite3`、未設定なら`~/.local/state/jrg/history-v1.sqlite3`です。`JRG_STATE_DIR`でディレクトリを変更できます。Unixではディレクトリ0700、DBと付随ファイル0600にします。

90日・全体10,000リクエスト・論理メタデータ64MiBを上限に自動整理します。DBのページやWALの物理サイズは別です。圧縮結果の行追跡には8,192行・原文4MiBの上限があり、省略分は`tracking_omitted`で示します。1件の履歴メタデータは64KiB以内です。超過する場合は順位補正の詳細を省き、保存できる完全な結果ID・範囲を残します。`metadata_omitted`に結果・補正詳細の省略件数と検索条件・対象パスの省略有無を記録し、stderrでも通知します。MCPへ返す評価対象IDは保存済みのものだけです。非常に大きい検索条件は元の条件の指紋と検索キーを保った要約にします。対象パス全体を保持できない記録は、安全のためMCPのhistory/feedbackから参照できず、CLIで確認します。履歴が開けない、ロックされているなどの場合はstderrへ通知し、検索を続行します。

`resume`は通常検索の必須パターンを展開せず、継続取得として記録します。評価による順位補正は行わず、返却した範囲と既読状態を更新します。空の継続取得後も同じMCP接続で検索できます。

OrgBrainは不要です。必要ならJSONL exportを上位側で取り込み、長期の知識やタスクの成果と関連付けられます。この実装はOrgBrainへの送信・同期を行いません。検索精度やタスク成功率の向上は、評価付きの実タスク比較で別途測定する必要があります。

## 実タスク比較

2026-10-17に、5課題・20調査で評価なしと314件のagent評価を比較しました。同じ検索手順で再実行した正常148応答は返却原文がすべて同じで、評価補正による精度・調査時間の改善は確認できませんでした。Embeddingは両条件とも無効です。

比較中に、履歴autoでの`resume`のpanicと、通常検索でも64KiB上限で履歴保存が失敗するケースを再現しました。2026-10-18に[回帰テスト付きで修正](HISTORY_FIXES_2026-10-18.md)しました。比較は修正前の固定版の結果です。条件・結果・ローカルの再現資料一覧は[実タスク比較レポート](../experiments/2026-10-17-history-feedback-ab/README.md)にあります。
