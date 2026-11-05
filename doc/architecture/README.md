# fkv 架构（as-built）

本目录描述当前 Rust 实现的架构与行为。

| 文档 | 主题 |
|---|---|
| [`01-overview.md`](./01-overview.md) | 分层与关键对象 |
| [`02-execution-model.md`](./02-execution-model.md) | shard / 请求 / 调度 |
| [`03-io-stack.md`](./03-io-stack.md) | IO trait 与后端 |
| [`04-btree-and-formats.md`](./04-btree-and-formats.md) | B+tree 与盘面格式 |
| [`05-persistence-and-recovery.md`](./05-persistence-and-recovery.md) | manifest 与恢复 |
| [`06-gaps.md`](./06-gaps.md) | 已知差异与缺口 |

约定：每篇只写“现在的代码怎么做”；未实现项进 `06-gaps.md`。
