# 迁移工具链规划（产品视角）

## 1. 用户与痛点

| 角色 | 关心什么 | 现状痛点 |
|------|----------|----------|
| 迁移工程师 | 今天做哪个 unit、依赖是谁、怎么算做完 | 1848 个 unit，靠对话记忆会丢上下文 |
| Reviewer | 这个 unit 为什么算迁完、有没有证据 | 只看代码看不出兼容性是否验证过 |
| PM / Owner | 进度、风险、兼容等级、性能 | LOC 不能反映真实进度 |

产品目标：

> 任何一个 C++ unit，都能查到它的状态、归属 crate、依赖、验证证据和下一步动作；不依赖某个人的记忆，也不依赖 AI 对话历史。

## 2. 程序化迁移流水线

```
baseline sync
  ├─ git fetch main，更新 COMPAT_BASELINE
  ├─ tree-sitter scan → unit / include_edge
  └─ path rule → crate / layer
        ↓
next / list / claim
        ↓
scaffold（生成 Rust 骨架 + TODO）
        ↓
fixture（C++ oracle 生成夹具）
        ↓
verify（V1–V4 差分 / 交叉读）
        ↓
evidence（写回账本）
        ↓
report（覆盖率 / 兼容率 / 性能）
```

原则：

1. **单一事实源**：motedb 账本 + `COMPAT_BASELINE` + fixtures。
2. **确定性 ID**：`path#kind:symbol`，重扫不会漂移。
3. **状态机驱动**：`todo → claimed → in_progress → review → done → verified`；`ignored` 终态。
4. **证据驱动**：没有 evidence 的 unit 不能标 `verified`。
5. **一切可 CI 复现**：命令、夹具、阈值都在仓库里。
6. **拒绝隐式知识**：不把“为什么跳过这个函数”留在聊天里，写进 `note`。

## 3. 工具清单

### 3.1 `tool/fkv_ledger`（已实现）

```bash
# 更新基线 + 扫描 + 映射
cargo run -p fkv_ledger -- sync

# 只看基线变化
cargo run -p fkv_ledger -- baseline --write

# 任务队列
cargo run -p fkv_ledger -- next --limit 20 --crate fkv_page
cargo run -p fkv_ledger -- list --status todo --crate fkv_page
cargo run -p fkv_ledger -- claim "<unit-id>" --owner z

# 状态 / 忽略
cargo run -p fkv_ledger -- status "<unit-id>" in_progress
cargo run -p fkv_ledger -- ignore "<unit-id>" --reason "inline helper"

# 证据 / 依赖
cargo run -p fkv_ledger -- evidence add "<unit-id>" diff --path fixtures/cpp/page.bin --result pass
cargo run -p fkv_ledger -- evidence list "<unit-id>"
cargo run -p fkv_ledger -- edges "<unit-id>"
```

数据模型：

- `unit`：迁移单元。
- `include_edge`：`#include` 依赖。
- `rule`：path → crate 映射。
- `evidence` / `claim` / `run` / `note`：证据、认领、运行、备注。
- `meta`：schema 版本。

### 3.2 `tool/fkv_ledger`（待补）

| 命令 | 作用 |
|------|------|
| `scaffold --crate fkv_page` | 按 unit 生成 Rust 模块骨架 + `TODO` + 测试文件名 |
| `fixture --crate fkv_page` | 调 C++ oracle 生成 page / manifest 夹具 |
| `verify --crate fkv_page` | 跑 V2/V3 差分，写 `run` / `evidence` |
| `report --markdown` | 生成 `doc/progress.md`：覆盖率、ignored、blocked、下一步 |
| `changed` | 列出 `source_hash` 变化的 unit，提醒重验 |
| `export --json` | 给外部 dashboard / 脚本用 |

### 3.3 C++ oracle

- 单独的 GitHub Actions：`.github/workflows/oracle.yml`。
- 只跑 `workflow_dispatch` / 定时，不阻塞每个 PR。
- 产物：page / manifest / store fixtures、`simple_bench` 结果。
- 本地 / 自托管 Linux 也可跑同一套 compose。

### 3.4 性能工具

- `crate/perf/fkv_bench`：criterion 微基准 + 端到端 workload。
- 指标同时记录：
  - **绝对值**：p50/p99、吞吐、RSS；
  - **相对比值**：同一 runner 上 Rust / C++。
- CI 门禁用相对比值，绝对值只做趋势。

### 3.5 CI

| workflow | 触发 | 内容 |
|----------|------|------|
| `ci.yml` | push / PR | fmt、clippy、nextest、`cargo check -p fkv_ledger`、compose config |
| `perf.yml` | 手动 / 每周 | Rust benchmark，上传 `target/criterion` |
| `oracle.yml` | 手动 / 每周 | 构建 C++ eloqstore，生成 fixtures / C++ bench |
| `compare.yml`（待补） | 下载两份 artifact | 算 Rust / C++ 相对比值，写 `run` 表 |

## 4. 如何避免“AI 交接”

- **任务不靠记忆**：`next` 从账本里按状态/优先级生成队列。
- **决策不靠聊天**：`ignore --reason`、`note` 落库。
- **进度不靠 LOC**：`stats` / `report` 看 verified / ignored / blocked。
- **兼容不靠感觉**：V1–V4 + fixtures 是硬证据。
- **上下文不靠长 prompt**：每个 unit 的 path、symbol、include、crate 都在账本里；新成员直接 `next`。
- **重扫不丢状态**：`scan` 按 ID upsert，保留 status / owner / evidence。

## 5. 指标

| 指标 | 定义 | 目标 |
|------|------|------|
| unit 覆盖率 | verified / (total - ignored) | 1.0 前 100% |
| 兼容通过率 | V2/V4 pass / 执行数 | 100% |
| 性能比值 | Rust / C++ p99、吞吐 | p99 ≤1.2×，吞吐 ≥0.85× |
| 平均 lead time | claimed → verified | 持续下降 |
| ignored 比例 | ignored / total | 有 reason，定期 review |
| blocked 数 | status = blocked | 每周清零 |

## 6. 路线图

| 阶段 | 交付 |
|------|------|
| P0（已完成） | sync / baseline / scan / map / next / list / claim / status / ignore / evidence / edges / stats |
| P1 | scaffold、fixture、verify、report |
| P2 | changed、export、compare workflow、dashboard |
| P3 | PR 自动评论账本变更、issue 同步 |

## 7. 产品决策

- 性能：绝对值和相对比值都记录；相对比值做门槛。
- 基线：跟 `main` 最新 commit；`sync` 一键更新 `COMPAT_BASELINE` + 重扫。
- Oracle：单独 GitHub Actions，不阻塞 PR。
- 账本：纯 CLI，先不做 Web UI / issue 同步。
- 兼容性：格式优先，V4 交叉读是 1.0 硬门槛。

## 8. 性能看板

- `web/`：Vite + Svelte + Pug + `@1-/stylus`；样式复用 webc 的 reset / form / button / card / kv。
- `web/src/App.svelte`：多语言看板；`zh` / `en`。
- `web/public/perf.json`：性能历史。
- `web/sh/tran.js` + `tran.yml`：复用 webc 翻译流程。
- `perf.yml`：x86_64 / aarch64 benchmark → 合并历史 → 构建 `web/dist` → 用 `GH_TOKEN` 推 `gh-pages`。
- `fkv_bench` binary：输出 `gxhash` 指标 JSON，后续接端到端 workload。
