# 実タスク比較中に再現した不具合

測定対象は `protocol.json` に固定したバイナリ
`de04e5d3b48692b50431722090d9ce0adf8fe3219bc8934e50fa3ed18d4345a6`。
比較中のコード・バイナリは変更していない。

## 1. 履歴有効時の継続取得でMCPが終了する

初期化済みのMCP接続で次を送ると再現する。

```json
{"method":"tools/call","id":2,"jsonrpc":"2.0","params":{"name":"retrieve","arguments":{"operations":[],"resume":true,"max_tokens":5000,"history_mode":"auto","context_id":"resume-diagnostic"}}}
```

- `auto`: 終了コード101。`src/cli.rs:234:48` の `Option::unwrap()` がpanicし、応答を返さない。
- `off`: 終了コード0。正常な空の継続取得結果を返す。
- 実際の `parcels-B-2` では継続取得で接続が落ち、その後の再検索も `Transport closed` になった。

`HistoryRuntime::begin` はbatch値がない場合に検索パターンを前提とする
`Args::inputs(true)` を呼ぶ。`resume` はパターンを要求しないため、そこからpanicする。
原文は `src/history_runtime.rs:58-84` と `src/cli.rs:230-235`。

再現結果は `resume_diagnostic.json`。この失敗を含む試行を時間短縮の成功例とは扱わない。
修正時は、継続取得で通常検索の必須パターンを展開しないことと、
保留範囲の取得後にも同じMCP接続が利用できることを回帰テストで確認する必要がある。

## 2. 通常の検索でも履歴メタデータが64KiBを超える

事前調査は40呼出しで、そのうち39呼出しが正常な検索・読取だった。
保存された履歴は30件。残り9件は同一条件の再実行でも
`request metadata exceeds the 64 KiB limit` を出して保存に失敗した。

| 課題 | 再実行時の上限超過数（各条件共通） |
| --- | ---: |
| smart経路 | 2 |
| 土地候補 | 0 |
| 音声処理 | 3 |
| 配布物 | 2 |
| 日本語検索 | 2 |

配布物の最初の通常検索でも単独で再現した。本文21,960文字の返却は成功するが、
履歴件数は0のまま。結果は `metadata_diagnostic.json`、全課題のstderrは
`replay_results.json` の各 `proxy_stderr` に残している。

履歴の追加メタデータが応答予算の都合で表示されない場合と、
DBへの保存自体が失敗する場合を区別する必要がある。
今回は正常呼出しの約23%で保存自体に失敗し、その結果には評価を付けられなかった。

上限は `src/history.rs:490`、保存前の拒否は同ファイルの `prepare`。
修正では、上限内に収めた範囲記録への縮約と省略状態の明示などを検討する。
上限を際限なく増やすことを前提にはしない。
