# jrg 0.15.2: MCP instructionsと4クライアントの優先ルール

2026-10-17 JST。追加の生成モデル・埋め込み推論は0。対象はこのjrgプロジェクト。別プロジェクトへのグローバル方針の変更は行っていない。

## 変更

- initialize応答にsrc/mcp_instructions.txtをinstructionsとして追加。先頭512文字に、jrgによるコード調査・内容検索・限定readの優先、通常rgによる全件列挙・不存在確認・正確な件数・機械出力・パイプ処理・未対応オプション/エンコード、利用不可/root外で権限内の通常ツールへ戻す方針を含めた。全体1441文字。retrieveの説明の先頭にも方針を追加。
- Codex/OpenCode: AGENTS.md。Claude Code: CLAUDE.mdからAGENTS.mdをimport。Cursor: .cursor/rules/jrg.mdcのalwaysApply=true。条件式の全節照合、省略の明示、限定追加取得、接続単位のseen/pendingを共通方針とした。
- Codexの既存.codex/config.tomlを保持。Claude .mcp.json、Cursor .cursor/mcp.json、OpenCode opencode.jsonを新規作成。同じ/bin/sh、scripts/jrg-mcp.sh、設定rootを使う。接続設定の絶対パスはこの端末用。
- Claudeの共有設定は初回承認待ちだったため、公式CLIのclaude mcp add --scope localでこのプロジェクト専用に登録し、Connectedを読戻した。プロジェクトの信頼設定や権限の全許可は変更していない。既存の他MCP設定は保持。
- 版を0.15.2へ上げ、release build後に普段の~/.local/bin/jrgとjrg-aiへ導入。旧0.15.1とラッパーはbackup/へ保全。以前の0.15.1の凍結成果物はそのまま維持。

## 検証

- initialize.instructionsを要求する期待動作テストの失敗を先に確認。追加後、Rust全136テスト・10スイート成功。cargo fmt --check、clippy --all-targets -- -D warnings、release build、diff check、ランチャーのsh -n成功。
- 4つの設定をパーサーで読み、それぞれの実コマンドを起動してMCP initialize/tools/list/retrieveを往復。全て0.15.2と同一instructionsを返した。条件式3節の原文、seen=0→6、2回目も条件式再表示、root外拒否を確認。
- 同じ要求のretrieve成功/拒否ペイロードを0.15.1へ渡した結果と完全一致で照合。比較対象はこの固定fixtureのMCP返却で、全利用条件やモデルの選択を網羅した主張ではない。
- Codexの設定読戻し成功。Claude公式CLIはjrg Connected。OpenCodeのdebug configでプロジェクト設定を認識し、mcp listでjrg connected。設定出力に含まれ得るprovider認証情報は出力・保存していない。
- Cursorは設定JSON・常時ルールのfrontmatter・その設定によるMCP往復を確認。Cursor UIでの接続・ルール読込みは未確認。UIのフォルダ選択で画面取得エラーが発生し、それ以降の操作は止めた。
- 現在のCodexチャットの既存MCP接続へ新instructionsを再配信したことは確認していない。Codex自体のUI操作はComputer Useの制限で拒否された。起動済み接続は再接続、新しい常時ルールはクライアントの再読込みまたは新しい会話で反映する。新規接続からの0.15.2動作は上記で検証済み。
- rgの実行パスとSHA-256は変更前後で一致。実際のrg --jsonの一致レコード1件、rg --quietの不存在終了コード1を確認。alias/rg実体や権限を置換・禁止していない。

導入後SHA-256: `2d34247bfe4eae581af765ac5be93e5bde61e9326037696e6eebcbd6de7e7003`
旧0.15.1 SHA-256: `3be2751524f261de3c9f0500897c53f6fea4b7f68cde96505c093d6460f52fa0`

証拠: green.jsonとruntime-green（全Rustテスト）、verification.jsonとconfig-check.json（4設定の通信と同等性）、claude-readback.txt、codex-readback.txt、opencode-readback.json、rg-before.json、manifest.json。

## 検証の限界

指示は優先方針を伝えるもので、モデルのツール選択を強制しない。モデルを追加実行していないので、各クライアントの実際の選択率・実利用の速度差は未測定。クライアントの既存会話のコンテキストへ新ルールが入ったとは断定しない。

## 参照した公式仕様

- OpenAI MCP instructions: https://developers.openai.com/codex/mcp
- Codex AGENTS.md: https://learn.chatgpt.com/docs/agent-configuration/agents-md
- Claude MCP/プロジェクトとlocal scope: https://code.claude.com/docs/en/mcp
- Claude imports: https://code.claude.com/docs/en/memory#import-additional-files
- Cursor alwaysApply: https://cursor.com/docs/rules
- OpenCode MCPとAGENTS.md: https://opencode.ai/docs/mcp-servers/ , https://opencode.ai/docs/rules/
