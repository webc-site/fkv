# C++ → Rust 迁移方法与账本

## 1. 调研结论

参考的真实迁移案例与可复用经验：

- **fish-shell C++ → Rust**：增量、master 常绿、从叶子组件开始、C++/Rust 长期共存、FFI 临时接受拷贝、优先行为保真而不是先写“idiomatic Rust”、避免长生命周期分支。
- **JetBrains C++ → Rust 迁移指南**：先接集成（构建、链接、CI、sanitizer），再从低依赖叶子模块开始；影子流量 / 双跑对比；逐步放量；保留回滚。
- **Strangler Fig**：不一次性重写；在旧实现旁边长出新区，按能力迁移，旧实现逐步退场。
- **差分测试**：同一输入同时喂给 C++ oracle 和 Rust 实现，比较输出；格式层比较字节，引擎层比较操作结果。
- **迁移账本**：按“单元”而不是按文件/行数统计；每个单元有状态、owner、依赖、验证证据；进度看“已验证单元数”，不看 LOC。

EloqStore 的特殊约束：

- 盘面格式必须双向可读（迁移闭环）。
- C++ 侧依赖 `boost::context`、absl、io_uring、curl，Crubit 不能全量自动绑定。
- Rust 侧用 compio，运行时模型与 C++ stackful coroutine 不同，不能 1:1 翻译。

## 2. 迁移原则

1. **格式优先**：先锁死 page / manifest / segment / term 的字节布局，再写引擎。
2. **读先于写**：Rust 先能读 C++ 写的 store，再写 Rust 自己的 store，再让 C++ 读 Rust 的输出。
3. **叶子优先**：`types → codec → page/manifest → mapping/btree → read/scan → write → recovery → maintenance → cloud/standby`。
4. **行为保真优先**：迁移期允许 Rust 写法接近 C++ 结构，方便对照；完成后再按 `rust_review` 重构。
5. **单 writer 交替**：C++ 与 Rust 不同时打开同一 store；只保证格式兼容，不保证跨语言崩溃恢复。
6. **master 常绿**：每个 crate 迁移完即可编译、可测试；不做长期分支。
7. **证据驱动**：每个迁移单元必须挂 evidence（测试、fixture、差分结果、benchmark），否则不算 done。
8. **oracle 冻结**：`COMPAT_BASELINE` 固定 eloqstore commit；升级基线时重新生成全部 fixtures。

## 3. `_cpp` 对照矩阵

Crubit 只覆盖 `cpp/shim` 的简单签名（`std::string_view`、`std::span<const uint8_t>`、POD、opaque `void*`），不直接吃 eloqstore 的复杂头文件。Crubit 生成失败时，降级为手写 C shim，但 `_cpp` crate 对外仍保持与 Rust crate 同形的 API。

| 档位 | crate | 验证方式 |
|------|-------|----------|
| **真 C++ 对照（Crubit）** | `fkv_types`、`fkv_codec`、`fkv_page`、`fkv_compress`、`fkv_manifest`、`fkv_mapping`、`fkv_btree`、`fkv_large`、`fkv_gc`、`fkv_compaction` | 同一输入，Rust / `_cpp` 输出逐字节或结构对比 |
| **C shim 对照** | `fkv_config`、`fkv_root`、`fkv_recovery` | 手写 shim + 内部 C 入口；结构等价对比 |
| **端到端 oracle** | `fkv_io_*`、`fkv_runtime`、`fkv_shard`、`fkv_task_*`、`fkv_cloud`、`fkv_standby`、`fkv_store`、`fkv_api`、`fkv_task`、facade | C++ 写 store → Rust 读/写 → C++ 读；比较操作结果 |
| **不做对照** | `fkv_metrics`、`fkv_failpoint`、`fkv_buf`、`fkv_testkit`、`fkv_bench`、`fkv_cli` | 只做功能测试 |

`_cpp` crate 只用于差分验证：

- 默认 `workspace.default-members` 包含空壳，但真实 C++ 实现用 feature `oracle` 门控。
- 生产依赖图禁止出现 `_cpp`。
- 构建需要 Linux + nightly + `rustc-dev` + Crubit；只在 Docker oracle 镜像里跑。

## 4. 验证阶梯

| 级别 | 内容 | 通过标准 |
|------|------|----------|
| V0 | 单元测试 | `cargo nextest` 全绿 |
| V1 | 格式 round-trip | 编码 → 解码 → 再编码，字节一致；proptest / fuzz |
| V2 | 格式差分 | 同输入，Rust 与 C++ 编解码结果一致 |
| V3 | 操作差分 | 同一 op log 分别跑 Rust / C++ store，读结果一致 |
| V4 | 交叉读 | C++ 写 → Rust 读/续写 → C++ 读；全量 scan 一致 |
| V5 | 崩溃恢复 | 各自实现 kill -9 后 replay 正确（不跨语言） |
| V6 | 性能 | 同机 Linux NVMe，对比 eloqstore `simple_bench` |

## 5. 迁移账本

工具：`tool/fkv_ledger`。数据库：motedb，目录 `.data/motedb.mote`（gitignore）。

### 5.1 数据模型

```sql
meta(k TEXT PRIMARY KEY, v TEXT)

units(
  id TEXT PRIMARY KEY,        -- path#kind:symbol
  path TEXT, symbol TEXT, kind TEXT,
  layer TEXT, crate TEXT, module TEXT,
  start_line INT, end_line INT,
  priority INT, risk TEXT,
  status TEXT,                -- todo|claimed|in_progress|review|done|verified|blocked
  owner TEXT,
  compat TEXT,                -- L0|L1|L2|L3|L4
  source_hash TEXT,
  created_at INT, updated_at INT
)

edges(id TEXT PRIMARY KEY, from_id TEXT, to_id TEXT, kind TEXT)  -- include|call|format|behavior
evidence(id TEXT PRIMARY KEY, unit_id TEXT, kind TEXT, path TEXT, result TEXT, commit_id TEXT, created_at INT)
claims(id TEXT PRIMARY KEY, unit_id TEXT, owner TEXT, status TEXT, branch TEXT, pr TEXT, claimed_at INT, released_at INT)
runs(id TEXT PRIMARY KEY, unit_id TEXT, suite TEXT, impl TEXT, result TEXT, metrics TEXT, commit_id TEXT, created_at INT)
notes(id TEXT PRIMARY KEY, unit_id TEXT, author TEXT, body TEXT, created_at INT)
```

### 5.2 状态机

```
todo → claimed → in_progress → review → done → verified
         ↑            ↓
         └── blocked ─┘
any → ignored（不需要迁移，终态）
```

- `claimed`：有 owner，未开始。
- `in_progress`：代码在写。
- `review`：clippy / test 通过，等审查。
- `done`：合并，缺 evidence。
- `verified`：evidence 齐全（差分 / 交叉读 / perf）。
- `blocked`：依赖未完成或决策未定。

### 5.3 当前 CLI

```bash
cargo run -p fkv_ledger -- db                    # 打开 / 初始化 .data/motedb.mote
cargo run -p fkv_ledger -- scan --root ../eloqstore
cargo run -p fkv_ledger -- list --status todo --crate fkv_page
cargo run -p fkv_ledger -- claim "<unit-id>" --owner z
cargo run -p fkv_ledger -- status "<unit-id>" in_progress
cargo run -p fkv_ledger -- ignore "<unit-id>" --reason "inline helper"
cargo run -p fkv_ledger -- stats
```

`scan` 已经用 tree-sitter 抽取 class / struct / function，并写入 `unit`。当前是纯 CLI；下一步补：

- `evidence add <unit> --kind --path --result`
- `edges` 抽取（include / call）
- `run` 写入（差分测试结果）
- `crate` 自动映射规则

### 5.4 单元粒度

- 头文件里的 `class` / `struct` / `enum`：一个 unit（`kind = class_specifier|struct_specifier|enum_specifier`）。
- 类内成员函数：每个方法一个 `method` unit；class 本身仍保留一个 unit。
- 文件作用域的 `function_definition`：一个 `function` unit。
- 类外定义的 `Foo::bar`：一个 `function` unit。
- 不需要迁移的函数：`ignore <unit> --reason ...`，状态置 `ignored`，rescan 保留。
- 宏 / 全局常量 / 文件级静态数据：单独 unit，`kind = macro|const`。

### 5.5 映射到 crate

`scan` 只填 `path/symbol/kind`；`crate/layer` 由人工或规则补：

- `include/storage/page.h` → `fkv_page`
- `include/storage/data_page.h` → `fkv_page`
- `include/storage/page_mapper.h` → `fkv_mapping`
- `include/storage/root_meta.h` → `fkv_root` / `fkv_manifest`
- `src/tasks/batch_write_task.cpp` → `fkv_task_write`
- `src/async_io_manager.cpp` → `fkv_io_compio`

后续在 `fkv_ledger` 里加一张 `rules(path_prefix, crate)` 表，自动映射。

## 6. 推荐顺序

1. **M0 脚手架**（已完成）。
2. **M1 格式层**：`fkv_types` / `fkv_error` / `fkv_codec` / `fkv_config` / `fkv_page` / `fkv_compress` / `fkv_manifest`。产出 C++ fixtures + V1/V2。
3. **M2 读路径**：`fkv_mapping` / `fkv_cache` / `fkv_btree` / `fkv_root` / `fkv_recovery` / `fkv_task_read` / `fkv_task_scan`。产出 V3/V4 读侧。
4. **M3 写路径**：`fkv_task_write` / `fkv_task_reopen`。产出 V4 闭环。
5. **M4 本地运行时**：`fkv_runtime` / `fkv_shard` / compaction / GC / archive / branch / TTL。
6. **M5 云与 standby**：`fkv_object_s3` / `fkv_cloud` / `fkv_standby` / term CAS / prewarm。
7. **M6 大 value / 零拷贝**：`fkv_large`、segment、managed buffer。
8. **M7 硬化**：fuzz、miri、loom、chaos、V5/V6。

## 7. 每个单元的 Definition of Done

- 代码合入，`cargo clippy --workspace --all-targets -- -D warnings` 通过。
- 单元测试 / 集成测试通过。
- 格式或算法单元：V1 + V2 evidence。
- 引擎路径单元：V3 或 V4 evidence。
- `fkv_ledger` 中状态到 `done`，evidence 行存在。
- 兼容等级 `compat` 更新。
- 文档 / 注释更新。

## 8. 参考

- fish-shell C++ → Rust 计划：<https://github.com/fish-shell/fish-shell/blob/master/doc_internal/fish-riir-plan.md>
- fish-shell Rust port 复盘：<https://fishshell.com/blog/rustport/>
- JetBrains C++ → Rust 迁移指南：<https://blog.jetbrains.com/rust/2026/07/27/cpp-to-rust-migration/>
- Strangler Fig：<https://martinfowler.com/bliki/StranglerFigApplication.html>
- Google `rs_bindings_from_cc` / Crubit：<https://github.com/google/crubit>
- motedb：<https://docs.rs/motedb/latest/motedb/>

## 9. CI 与性能

- `.github/workflows/ci.yml`：`ubuntu-26.04` / `ubuntu-26.04-arm`，fmt / clippy / nextest / `cargo check -p fkv_ledger`；action 用最新 major（checkout v7、upload-artifact v7、rust-cache v2、install-action v2）。
- `.github/workflows/perf.yml`：只在原生 Linux 上跑 `cargo bench -p fkv_bench --bench hash`，上传 `target/criterion`；共享 runner，取相对比值。
- `.cargo/config.toml`：gxhash 无运行时回退，x86_64 加 `+aes,+sse2`，aarch64 加 `+aes,+neon`。
- 独立性能 crate：`crate/perf/fkv_bench`。
