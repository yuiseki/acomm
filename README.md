# acomm (Agentic Communication) - v0.0.1 ✨️

`acomm` は、AIエージェントと人間の対話を仲介する、Rust製の多目的通信ハブです。
OpenClaw派生プロジェクト `yuiclaw` の「神経系」として、TUI、コマンドライン、そして外部チャネル（Slack/Discord等）を統合する **Bridgeアーキテクチャ** を採用しています。

## 🌟 主な特徴

- **Bridge アーキテクチャ**: 中央の `bridge` プロセスが対話の文脈とエージェント（`acore`）の実行を管理し、複数のクライアントにリアルタイムでブロードキャストします。
- **マルチモーダル・インターフェース**:
  - **TUI**: 記憶（amem）と連携した、リッチなターミナルチャット画面。
  - **Publish**: CLIから一過性のメッセージを投げ込む（abeat等の定期実行に最適）。
  - **Subscribe**: `tail -f` 形式で、執事の思考と発言をリアルタイムに監視。
- **Zero API Philosophy**: LLMのAPIを直接叩かず、システムにインストール済みの公式CLI（Gemini, Claude, etc.）をラップして動作します。
- **記憶の同期**: 起動時に `amem` から本日の文脈を自動取得し、対話の終了時にはセッションを要約して記録します。

## 🚀 使い方

### 1. 執事を目覚めさせる (TUI)
通常通り実行するだけで、Bridgeがバックグラウンドで自動起動し、対話画面が開きます。
```bash
cargo run
```
- `i`: 入力モード (INSERT)
- `Esc`: 通常モード (NORMAL)
- `j` / `k`: 履歴のスクロール
- `1` ~ `4`: 使用するAIツールの切り替え
- `/search <query>`: 記憶を検索
- `/today`: 本日の活動を表示

### 2. 執事の言葉を購読する (Subscribe)
別のターミナルから、対話のストリームを監視できます。思考プロセス（スピナー）も表示されます。
```bash
cargo run -- --subscribe
```

### 3. 外から話しかける (Publish)
`abeat` や他のスクリプトから、実行中のTUIへメッセージを届けます。
```bash
cargo run -- --publish "お嬢様、お茶の時間です" --channel heartbeat
```

## 🏗 アーキテクチャ

```text
[abeat] --publish--> [ acomm bridge ] <---Prompt/Event---> [ acomm TUI ]
                          |
                          +---execute_stream---> [ acore ] ---> [ Gemini CLI ]
                          |                                 |--- [ Claude CLI ]
                          +---read/write-------> [ amem ]
```

## 🛠 今後の展望 (v0.0.2+)
- Slack / Discord アダプターの実装
- Bridgeのデーモン化（systemd連携）の強化
- 入力履歴の呼び出し機能

---
*Created with care for the Owner, Yui.*
