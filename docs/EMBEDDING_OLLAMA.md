# Mac/Codex のローカル埋め込み検索

この設定は、Mac 上の Ollama にある `embeddinggemma-2:270m-bf16-text` を、jrg の `review` 検索から使う。追加のモデルや Python パッケージは取得しない。

## 起動とモデルの保持

- `.codex/config.toml` が `JRG_EMBEDDING_ENABLED=1` を渡し、`scripts/jrg-mcp.sh` が MCP 接続ごとの Unix ソケットブリッジを起動する。
- ソケットは接続ごとの一時ディレクトリに作り、新しい接続では再利用しない。起動失敗時は `JRG_EMBEDDING_SOCKET` を設定せず字句検索へ戻る。親プロセスが異常終了した場合、ブリッジは処理中の要求を終えてから最大 5 秒後に終了する。
- ブリッジは起動時にモデルを読み込まない。jrg が埋め込み検索を要求したとき、Ollama の `POST /api/embed` が初めてモデルをロードする。
- 各埋め込み要求は `keep_alive=5m` を指定する。Ollama は同じモデルへの最後の要求から 5 分後にメモリーを解放し、次の要求で再ロードする。OrgBrain もこのモデルを共有するため、タイマーは両方の利用を含む。jrg は Ollama 全体の保持設定を変更せず、明示的な unload も送らない。
- 使用するタグは `embeddinggemma-2:270m-bf16-text`、ダイジェストは `afeb999480b2a5e48b590fc0583f8cbc7d206b74156feaead3a7840b81719295`。埋め込み要求の形式と保持時間は[Ollama の API 仕様](https://github.com/ollama/ollama/blob/main/docs/api.md#generate-embeddings)に従う。
- ローカルの Ollama タグは SHA-256 ダイジェストで固定する。タグがない、またはダイジェストが違う場合は、検索結果を不正なものとして扱い jrg が字句検索へ戻る。
- 既存インストール済み jrg バイナリとの互換性のため、Unix 応答には従来の `google/embeddinggemma-2` revision を維持する。実行時の Ollama artifact は別の完全な SHA-256 で検証し、SQLite のキャッシュキーにもその Ollama digest を使う。
- `Document` と `CodeRetrieval` の入力には[固定した Google モデル版](https://huggingface.co/google/embeddinggemma-2/blob/914f7f89142e33e77833254d9c9b90c3cef7303b/README.md)の公式タスク接頭辞を付け、768 次元の有限なベクトルだけを受け付ける。

## キャッシュとデータ

埋め込みベクトルの SQLite キャッシュは `~/Library/Caches/jrg-embedding/vectors.sqlite3` に保存する。キャッシュキーには固定モデル、プロンプト、ウィンドウ方式と原文内容のハッシュを含める。モデルがメモリーから解放されてもキャッシュは残り、ファイルを編集すると次の検索で変更部分を自動計算する。

Ollama に送るのは localhost 内の埋め込み要求だけ。`runtime.jsonl` にはモデル識別子、件数、所要時間とエラー種別を記録し、検索語・ファイルパス・原文は記録しない。ディレクトリとファイルは所有者だけが読める権限で作る。

Ollama モデルは OrgBrain も使う共有モデルである。jrg は OrgBrain の設定を変更せず、モデルを削除・強制アンロードしない。

## 実機確認

2026-10-18 に Python の埋め込みテスト9件と Rust semantic テスト3件が通過した。Codex の実際の `jrg.retrieve` は `embedding=google/embeddinggemma-2` を返した。`runtime.jsonl` の 05:16:47 UTC の検索記録では、7チャンクの文書ベクトルがキャッシュ済みで、クエリ1件だけを埋め込み、合計約24msだった。別の 04:45:19 UTC の実検索では7文書を埋め込み、モデルの `load_duration` は約0.71秒だった。

05:16:47 UTC が最後の jrg 検索記録である一方、05:29:23 UTC の `/api/ps` ではモデルがロード中で、有効期限は14:34:11 JSTだった。Ollama モデルは OrgBrain と共有されるため、この確認では5分間の完全なアイドル解放とその後の再ロードを切り分けられなかった。強制アンロードは行っていない。

## 無効化と削除

`.codex/config.toml` から `env = { JRG_EMBEDDING_ENABLED = "1" }` を削除するか、値を `"0"` にすると、jrg は Unix ソケットを設定せず字句検索を使う。計測ラッパーはそのまま有効。

キャッシュも削除する場合は、jrg MCP 接続を閉じてから `~/Library/Caches/jrg-embedding` を削除する。共有 Ollama モデルは OrgBrain も利用するため、jrg の削除作業では Ollama から削除しない。
