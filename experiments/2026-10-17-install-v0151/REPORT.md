# 0.15.1の普段使いへの導入とMCP動作確認

2026-10-17 JST。追加の生成・埋め込みモデル実行なし。CLI対MCPの速度比較は実施せず、実利用後の評価へ延期した。

- 導入元: `experiments/2026-10-17-integration-cache/jrg-0.15.1`。既存の検証済み成果物を指定して `install.sh --binary` を実行し、再ビルドしていない。
- 導入先: `/Users/miyajimakazuhiro/.local/bin/jrg`。通常のPATHで解決する `jrg` と `jrg-ai` は両方 `jrg 0.15.1` を返す。旧版は0.8.0。旧バイナリとラッパーを本ディレクトリの `backup/` に保全。
- SHA-256: `3be2751524f261de3c9f0500897c53f6fea4b7f68cde96505c093d6460f52fa0`。導入元、導入先、検証済みprotocolのcandidate_sha256が一致。
- 日本語検索: 通常CLIの `--ai` と `jrg-ai` で、`ガッコウ` / `ｶﾞｯｺｳ` を `がっこう` で2件取得し、省略・切詰めなし。
- MCP設定: このプロジェクトの `.codex/config.toml`。グローバル設定にjrgを追加せず、他プロジェクトへ固定rootを持ち込まない。`scripts/jrg-mcp.sh` が導入先バイナリを起動。接続ごとにmktempの状態領域を作り、終了時に削除する。セッションは接続中の呼出しで共有され、別接続や旧版と共有しない。再接続時は取得済み状態を引き継がない。
- `codex mcp get jrg --json` で有効な設定を読戻し。このチャットで認識された `mcp__jrg__retrieve` を2回実行。R4の対応表でASTによる3節すべての条件式を返す。2回目は `seen=6`、通常原文は抑制、条件式は再表示。両方 `pending=0` / `errors=0` / `conditions_incomplete=0` / `omitted=0`。
- 稼働中プロセスの実引数とlsofのtxtマッピングでも、`~/.local/bin/jrg` の使用を確認した。設定表示だけの確認ではない。
- ランチャーは実装前のテスト失敗を確認後に実装。同一接続のseen、条件式の再表示、2つの接続間の状態独立、MCP initializeの0.15.1を検証して成功。`git diff --check` 成功。

証拠: `before.json`, `cli.json`, `direct-mcp.json`, `live-mcp.json`, `live-process.json`, `test_launcher.py`。

補足: 独立したCodex app-serverによる補助検査は、検証スクリプトの設定キー指定を修正後、通常サンドボックスのOS権限で起動できなかった。代替としてこのチャットで認識された本物のMCPツールを呼び出し、稼働中プロセスの実体と合わせて確認した。補助検査の失敗を接続成功の証拠として扱っていない。
