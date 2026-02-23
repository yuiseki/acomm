# Yuiclaw Project Milestones

YuiClawは、複数のAIエージェントCLIを統括し、あらゆるチャネルで執事との対話を実現する「Agent Communication Hub」です。

## Milestone 1: Foundation & TUI (CURRENT FOCUS)
基礎アーキテクチャの確立と、デスクトップにおける至高の対話体験の実現。

- [x] **Bridge Architecture**: Unix Domain Socketを用いた、UIsとAgent Coreの分離。
- [x] **TUI Dashboard**: Ratatuiによる、リアルタイムストリーミング・日本語対応・履歴機能付きインターフェース。
- [x] **Stateful Session**: Session IDの抽出とResume機能による、ツールを跨いだ文脈の維持。
- [x] **Daily Logging**: ~/.cache/acomm/sessions/ へのJSONL形式による自動会話記録。
- [x] **amem Integration**: 記憶（amem）からの動的なコンテキスト注入。
- [ ] **Test Fortification**: 統合テストの安定化と、エッジケース（ネットワーク断、不正なJSON等）の網羅。
- [ ] **Comprehensive Documentation**: プロトコル詳細および開発者ガイドの整備。

## Milestone 2: Multi-Channel Expansion
外出先からのアクセスと、エージェント間の連携強化。

- [ ] **Slack Adapter**: Socket Modeを用いた双方向通信。
- [ ] **Discord Adapter**: サーバー/DMでの執事呼び出し。
- [ ] **Proactive Notifications**: ntfy.shを用いた、重要イベントのプッシュ通知。
- [ ] **Auto-Summary**: 会話終了時のamemへの自動活動記録の更なる洗練。

## Milestone 3: Intelligent Orchestration
エージェント自身の「意思」によるツール選択と、自律的な課題解決。

- [ ] **Tool Routing**: プロンプト解析による、最適なAIツール（Gemini/Claude等）の自動選択。
- [ ] **Cross-Agent Collaboration**: 複数のAIが連携して一つの課題を解決するワークフロー。
- [ ] **Memory Pruning**: 膨大な記憶（amem）からの、適切な情報抽出と要約。
