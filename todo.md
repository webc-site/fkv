# fkv 实施 TODO（Rust 版 EloqStore）

> 目标：在 `./`（fkv workspace）实现与 eloqstore 同架构、同语义、可对标性能的 Rust 存储引擎。
> 参考：`../eloqstore`（架构 / 盘面格式 / 行为）、`./.agents/skills/rust_review/SKILL.md`（编码规范）。
> 运行时：compio 0.19（启用 `macros`），thread-per-core。
> Crate 命名：统一 `fkv_` 前缀（如 `fkv_types`）；facade 为 `fkv`；C++ 对照为 `<name>_cpp`（如 `fkv_types_cpp`）。
> 当前状态：设计定稿 v0.3；盘面“与 eloqstore 字节兼容”，Apache-2.0，1.0 全功能，迁移方法与账本见 `./doc/migration.md`。

---

## 0. 结论先行

- **44 个 Rust crate，6 层（另 44 个 `_cpp` 对照）**（L0 基础 → L5 外观），只允许向下依赖，`xtask` 强制检查。
- **运行时模型**：1 shard = 1 OS 线程 = 1 个 compio Runtime；分区按 hash 静态路由；shard 内单线程无锁，跨线程只走 MPSC + waker。
- **任务模型**：async/await future 替代 C++ 的 stackful coroutine；任务取消 = drop future，pin / snapshot 用 RAII 释放。
- **四条硬约束**：
  1. shard-thread affinity 替代锁；
  2. COW B+tree，读写不互斥；
  3. manifest 是唯一 durability anchor；
  4. 热路径禁 `dyn`，用 enum dispatch / 泛型。
- **1.0 范围**：Local + Mem + Cloud + Standby；Read / Floor / Scan / BatchWrite / Truncate / Drop / Reopen；Branch / Archive / Term / TTL；large value / segment / 零拷贝。按里程碑分阶段交付。
- **盘面格式**：与 eloqstore **字节兼容**（双向可读）：文件命名、目录布局、page / manifest / segment / term 文件互相可解析；不要求同 workload 逐字节一致。
- **兼容基线**：锁定当前 `eloqstore` 仓库 commit，写入 `COMPAT_BASELINE`；1.0 不跟进上游格式变更。

---

## 1. 目标与非目标

### 1.1 目标

1. **API 对标**：Rust SDK 覆盖 `EloqStore` / `Options` / `TableIdentifier` 的 `put/get/delete/scan`，并提供 async request/response API。
2. **模式对标**：Local、Mem、Cloud、StandbyMaster、StandbyReplica 五种 StoreMode，选项语义一致。
3. **核心机制对标**：COW B+tree、全内存 index page、4 KiB page、prefix compression + restart、TTL tree、manifest snapshot/log、term、branch、archive。
4. **盘面字节兼容**：C++ 能打开 Rust 写的 store，Rust 能打开 C++ 写的 store；按 `doc/migration.md` 的 V4 交叉读逐级验收。
5. **性能对标**：冷数据点读恰好 1 次磁盘 IO；批量写不阻塞读；同硬件下 p99 与 eloqstore 同量级。
6. **崩溃一致性**：任意 kill point 后可 replay 到最后一条完整 manifest record；各自实现恢复自己的 store，不要求跨语言崩溃恢复。
7. **可嵌入**：Rust SDK，保持 eloqstore 的生命周期 / 完成 / 所有权模型。
8. **全功能 1.0**：Cloud / Standby / Branch / Archive / TTL / large value / zero-copy 全部交付；不做 Python SDK。

### 1.2 非目标（1.0 之前不做）

- RESP / MySQL / MongoDB 协议服务端。
- Raft / 多进程 / 分布式协调。
- **双进程同时写同一个 store**：只保证单 writer、交替打开的互操作。
- **跨语言崩溃恢复**：C++ 写一半崩溃后 Rust 接手恢复不作为验收项；各自 replay 自己的 store。
- **Python SDK / 公开 C ABI**：都不做；`cpp/shim` 的内部 C 入口只给 oracle 用。
- **同 workload 逐字节一致**：zstd / 文件 ID / 并发调度不可复现；只要求双向可读 + 逻辑等价。
- GCS / Azure 对象存储（`ObjectStore` trait 预留，只实现 S3 兼容）。
- Windows / IOCP（compio 支持，但 1.0 只验收 Linux io_uring；macOS polling 仅开发用）。
- 向量索引、SQL 层。
- eloqstore 的 `ELOQ_MODULE_ENABLED` 宿主调度模式（1.0 不做，接口预留）。

### 1.3 对标口径

| 维度 | 口径 |
|------|------|
| 硬件 | 同一台 Linux NVMe 机器，关闭 CPU 频率漂移 |
| 负载 | 复用 eloqstore `benchmark/simple_bench` 的 write-read / kv_size / batch_size / max_key / partitions |
| 指标 | 吞吐、p50/p99/p999、RSS、写放大、云模式上传字节数 |
| 正确性 | 与 eloqstore 跑同一 workload 后全量 scan 比对 |

---

## 2. 架构映射

| eloqstore 概念 | fkv 对应 | 说明 |
|----------------|----------|------|
| `EloqStore` | `fkv_store::Store` | 生命周期、模式派生、路由、global fanout |
| `Shard` + `WorkLoop` | `fkv_runtime` + `fkv_shard` | OS 线程 + compio Runtime + 请求队列 |
| `KvTask` / `TaskManager` | `fkv_task` + `fkv_task_*` | future 任务、任务注册表、每分区写队列 |
| `AsyncIoManager` | `fkv_io::IoManager`（enum） | trait + enum dispatch，无 `dyn` |
| `IouringMgr` | `fkv_io_compio` | compio-fs / io_uring，FD LRU、O_DIRECT、合并写 |
| `MemStoreMgr` | `fkv_io_mem` | 纯内存后端 |
| `CloudStoreMgr` | `fkv_cloud` | 整文件 cache + 字节预算 LRU + S3 overlay + term CAS |
| `StandbyStoreMgr` + `StandbyService` | `fkv_standby` | rsync / ssh 拉取 |
| `Page` / `MemCachedPage` | `fkv_page` / `fkv_cache` | 盘面编码与内存缓存分离 |
| `PageMapper` / `MappingSnapshot` | `fkv_mapping` | 逻辑页 → 物理页，版本化快照 |
| `RootMeta` / `RootMetaMgr` | `fkv_root` | 分区持久状态 + 全局 LRU |
| `Replayer` | `fkv_recovery` | 冷启动、reopen、remote snapshot install |
| `FileGc` | `fkv_gc` | retained set、BranchGuard、archive 剪枝 |
| `BatchWriteTask::Apply` | `fkv_btree` + `fkv_task_write` | COW 合并 / 分裂算法与任务编排分离 |
| `GlobalRegisteredMemory` / segments | `fkv_large` | 大 value、segment mapping、metadata trailer |
| `rust/eloqstore` | `fkv_sdk` | Rust 高层 SDK；不做 C ABI |

---

## 3. Crate 拆分

### 3.1 分层与拆分准则

- **只允许向下依赖**：L0 → L5，禁止同层横向依赖；`cargo xtask deps` 检查。
- **一个 crate = 一个编译边界 + 一个所有权边界**：格式、IO、任务、算法各自独立，便于单测与 feature 裁剪。
- **`pub` 面最小化**：内部结构体直接暴露字段；除 `fkv` facade 外禁止二次导出依赖。
- **拆分阈值**：稳定后 < 800 LOC 的 crate 合并回父 crate；> 8k LOC 的 crate 再按子领域拆。避免为拆而拆。
- **测试位置**：集成测试在 `tests/`，`src/` 内只放必要的单元测试；禁止常量 == 常量这类废话测试。
- **feature 可加性**：feature 只做加法，不允许改变已有 API 语义；`--all-features` 必须可编译。

### 3.2 目录布局

```
fkv/
├── Cargo.toml                 # workspace, resolver = "3", edition = "2024"
├── rust-toolchain.toml        # nightly（fmt / clippy / rustc-dev for Crubit）
├── rustfmt.toml               # 复用 fkv 配置
├── clippy.toml
├── deny.toml
├── xtask/                     # deps 检查 / crate 生成 / 迁移账本入口
├── crate/
│   ├── foundation/            # L0；每个目录含 <crate> 与 <crate>_cpp
│   ├── format/                # L1
│   ├── core/                  # L2
│   ├── io/                    # L3
│   ├── engine/                # L4
│   ├── facade/                # L5
│   └── test/                  # testkit / bench
├── cpp/
│   ├── shim/                  # 给 Crubit 的简单类型 C++ 包装
│   ├── eloqstore/             # 只读引用 ../../eloqstore（submodule 或挂载）
│   └── build/                 # CMake 产物，gitignore
├── tool/
│   └── fkv_ledger/            # motedb + tree-sitter 迁移账本
├── docker/
├── web/                       # GitHub Pages 性能看板
│   ├── docker-compose.yml     # oracle / rustfs / test / bench
│   ├── Dockerfile.oracle      # Ubuntu 26.04 + eloqstore 依赖
│   └── Dockerfile.test        # Rust nightly + rustc-dev + Crubit
├── fixtures/
│   ├── cpp/                   # C++ oracle 生成的 page / manifest / store 夹具
│   └── rust/                  # Rust 生成、回喂 C++ 的夹具
├── .data/                     # gitignore：motedb / store / bench 数据
└── fuzz/                      # cargo-fuzz，不作为 workspace member
```

`.gitignore` 必须包含：`target/`、`.data/`、`data/`、`*.db`、`*.mote`、`cpp/build/`、`fixtures/generated/`。

每个 crate 目录并排一个 `_cpp` 对照 crate：

```
crate/format/fkv_page/          # Rust 实现
crate/format/fkv_page_cpp/      # Crubit 绑定的 C++ 对照实现
```

- `_cpp` crate 只用于 **差分验证**，不进入生产依赖图；默认 `workspace.exclude` 或 feature `oracle` 才构建。
- `_cpp` crate 的 `src/lib.rs` 暴露与 Rust crate 同形的函数；内部通过 Crubit 生成的绑定调用 `cpp/shim`。
- 无法用 Crubit 覆盖的类型，收敛到 `cpp/shim` 的简单签名（`void*` / `std::string_view` / POD），不让 Crubit 直接吃 eloqstore 的复杂类型。

### 3.3 L0 基础层

| crate | 职责 | 关键依赖 | 阶段 |
|-------|------|----------|-----|
| `fkv_types` | `TableIdent`、`PageId`、`FilePageId`、`FileId`、`Term`、`DsShardId`、`StoreMode`、`WriteOp`、`BranchFileRange`、`RetainedFileKey`、文件名常量 | `hipstr` | ✅ |
| `fkv_error` | `KvError` 扁平枚举、`Result`、`IsRetryableErr`、错误上下文 | `thiserror` | ✅ |
| `fkv_codec` | LE 读写、varint32/64、bit flags、`XXH3_64bits`、`zerocopy` 固定头 | `xxhash-rust`、`zerocopy` | ✅ |
| `fkv_config` | `KvOptions` 全字段、INI 手写 FSM 解析、校验与 rewrite、`store_path` LUT | `fkv_types`、`fkv_error` | ✅ |
| `fkv_metrics` | 计数器 / gauge / 直方图、shard 级 meter、MeteredIo | `std::time`、`fkv_io` | ✅ 首版 |
| `fkv_failpoint` | 命名 fail point / kill point 注册表，仅测试 feature | 无 | M2 |

### 3.4 L1 格式层

| crate | 职责 | 关键依赖 | 阶段 |
|-------|------|----------|-----|
| `fkv_page` | `PageType`、Data / Index / Overflow 页 builder + iterator、restart、prefix compression、checksum | `fkv_types`、`fkv_error`、`fkv_codec` | ✅ |
| `fkv_compress` | zstd 字典训练 / 复用、单值压缩解压、`DictCompression` | `fkv_error`、`zstd` | ✅ |
| `fkv_manifest` | snapshot / log record framing、mapping delta、`BranchManifestMetadata`、archive 命名 | `fkv_types`、`fkv_error`、`fkv_codec` | ✅ |

> `fkv_manifest` 只处理字节与 delta，不依赖 `fkv_mapping`，由 `fkv_recovery` 把 delta 应用到映射表，避免循环依赖。

### 3.5 L2 内存 / 树层

| crate | 职责 | 关键依赖 | 阶段 |
|-------|------|----------|-----|
| `fkv_buf` | 4 KiB / 可变长对齐 buffer、`PagesPool`、`DirectIoBuffer`、managed buffer pool | `compio-buf`、`fkv_error` | ✅ |
| `fkv_mapping` | `MappingTbl`（`Cell<u64>` 分块）、`ValType`、`MappingSnapshot`、`PageMapper`、Append / Pooled allocator | `fkv_types`、`fkv_error`、`gxhash` | ✅ |
| `fkv_cache` | `MemCachedPage` 槽位、LRU、pin / `Handle`、swizzle、等待座位 | `fkv_buf`、`fkv_page`、`fkv_mapping` | ✅ |
| `fkv_btree` | `SeekIndex` / `SeekFloor`、COW 合并 / 分裂、write stack、叶子链表维护 | `fkv_page`、`fkv_cache`、`fkv_mapping` | ✅ |
| `fkv_root` | `RootMeta`、`RootMetaMgr` LRU、`first_unflushed_*`、压缩字典 | `fkv_mapping`、`fkv_manifest`、`fkv_cache` | ✅ |
| `fkv_large` | segment mapping、大 value 编码 / 解码、metadata trailer、读写计划 | `fkv_page`、`fkv_mapping`、`fkv_codec`、`fkv_io` | ✅ 首版 |

### 3.6 L3 IO 层

| crate | 职责 | 关键依赖 | 阶段 |
|-------|------|----------|-----|
| `fkv_io` | `PageIo` / `ManifestIo` / `FileIo` / `SegmentIo` / `FixedSegmentIo` / `TermIo` trait；`FixedBuffer`；`enum IoManager` | `fkv_types`、`fkv_error`、`fkv_buf`、`fkv_page`、`fkv_manifest` | ✅ |
| `fkv_io_mem` | 纯内存后端 | `fkv_io` | ✅ |
| `fkv_io_compio` | compio-fs 后端：open / read_at / write_at / sync_data / rename / read_dir、FD LRU、O_DIRECT、合并写 | `fkv_io`、`compio` | ✅ |
| `fkv_io_sim` | 确定性模拟后端：延迟、故障、crash、重排 | `fkv_io`、`fkv_failpoint` | M2 |
| `fkv_object` | `ObjectStore` trait、key 规范、List 分页、conditional put | `fkv_types`、`fkv_error` | M4 |
| `fkv_object_s3` | S3 SigV4 + HTTP + ListObjectsV2；RustFS / S3 兼容 | `fkv_object`、`object_store[aws]`、`tokio` | M4 |
| `fkv_standby` | 角色、远端 list / fetch、manifest stage、rsync / ssh 骨架 | `fkv_config`、`fkv_manifest`、`std::process`（可替换 compio-process） | ✅ |

### 3.7 L4 引擎层

| crate | 职责 | 关键依赖 | 阶段 |
|-------|------|----------|-----|
| `fkv_api` | Request / Response 枚举与结构体、完成契约、sync wrapper、LargeValue 目标 | `fkv_types`、`fkv_error`、`fkv_buf`、`fkv_large` | ✅ |
| `fkv_task` | `Task` trait、`TaskCtx`、等待座位 / 协程锁、yield / 优先级门、重试策略 | `fkv_api`、`fkv_io`、`fkv_metrics` | ✅ |
| `fkv_runtime` | shard OS 线程、compio Runtime 构建、CPU 亲和、请求 ingress、shutdown | `fkv_task`、`fkv_config`、`fkv_metrics` | ✅ |
| `fkv_shard` | work loop、admission、每分区写队列、任务注册表、定时器、关停 | `fkv_runtime`、`fkv_task`、各 `fkv_task_*` | ✅ |
| `fkv_task_read` | Read / Floor | `fkv_task`、`fkv_btree`、`fkv_cache`、`fkv_root` | ✅ |
| `fkv_task_scan` | Scan 迭代、prefetch、分页 | `fkv_task`、`fkv_btree`、`fkv_cache`、`fkv_page` | ✅ |
| `fkv_task_write` | BatchWrite / Truncate / Drop / TTL | `fkv_task`、`fkv_btree`、`fkv_root`、`fkv_manifest` | ✅ |
| `fkv_task_reopen` | Reopen、auto-reopen、快照安装 | `fkv_task`、`fkv_recovery` | ✅ |
| `fkv_task_compact` | Compaction 任务编排 | `fkv_task`、`fkv_compaction` | M3 |
| `fkv_task_gc` | FileGc / LocalGc 任务编排 | `fkv_task`、`fkv_gc` | M3 |
| `fkv_task_archive` | Archive / CreateBranch / DeleteBranch | `fkv_store` 首版内联实现 | ✅ 首版 |
| `fkv_task_prewarm` | 云缓存预热：manifest / data / segment 拉取 | `fkv_cloud`、`fkv_io`、`fkv_object` | ✅ 首版 |
| `fkv_recovery` | `Replayer`、冷启动、`InstallExternalSnapshot`、manifest rotate | `fkv_manifest`、`fkv_mapping`、`fkv_root`、`fkv_io` | ✅ |
| `fkv_gc` | retained set、BranchGuard、archive 剪枝、local / cloud GC 计划 | `fkv_mapping`、`fkv_manifest`、`fkv_io`、`fkv_object` | M3 |
| `fkv_compaction` | 空间放大分析、页重写计划、segment 重写计划 | `fkv_page`、`fkv_mapping`、`fkv_root` | M3 |
| `fkv_cloud` | 云 overlay：整文件 upload / download、cache 预算、term CAS、RefreshManifest | `fkv_io`、`fkv_object`、`fkv_config`、`fkv_gc` | M4 |
| `fkv_store` | `Store` 生命周期、模式派生、路由、global fanout、Attach / Detach、TTL、Truncate / Drop、大 value | `fkv_runtime`、`fkv_shard`、`fkv_cloud`、`fkv_standby` | ✅ |

> 任务 crate 不依赖 `fkv_shard`，只依赖 `fkv_task` 的上下文 trait；由 `fkv_shard` 依赖任务 crate 并构造。这样避免 `shard ↔ tasks` 循环依赖。

### 3.8 L5 外观 / 工具 / 测试

| crate | 职责 | 关键依赖 | 阶段 |
|-------|------|----------|-----|
| `fkv` | 唯一 curated facade、examples、`#[compio::main]` 示例 | `fkv_store`、`fkv_api`、`fkv_config` | ✅ |
| `fkv_sdk` | `put/get/delete/scan` 便捷 API + typed request/response trait，对齐 eloqstore Rust SDK | `fkv` | M6 |
| `fkv_cli` | `simple_bench` 等价物、page / manifest 检查工具 | `fkv`、`clap` | M2 |
| `fkv_testkit` | 临时目录、页 / manifest 夹具、确定性时钟、crash harness | 全部 | ✅ |
| `fkv_bench`（`crate/perf/`） | criterion 微基准 + 端到端 workload | `fkv`、`criterion`、`gxhash` | M6 |
| `fuzz/` | page / manifest / codec fuzz target（cargo-fuzz，不加入 workspace） | `fkv_page`、`fkv_manifest` | M1 |

### 3.9 依赖 DAG

```
L0  types   error   codec   config   metrics   failpoint
L1            page   compress   manifest
L2     buf   mapping   cache   btree   root   large
L3     io ── io-mem / io-compio / io-sim
       object ── object-s3
       standby
L4     api   task   runtime   shard
       task-read / scan / write / reopen / compact / gc / archive / prewarm
       recovery   gc   compaction   cloud   store
L5     fkv   sdk   cli   testkit   bench
```

规则：

- `fkv_io` 定义 trait + `IoManager` enum；backend 只依赖 trait，引擎只依赖 enum。
- `fkv_store` 是唯一认识所有 backend 的 crate，负责装配。
- `fkv_cloud` 不直接依赖 `fkv_io_compio`；通过 `fkv_io` 的 trait 访问本地 cache。
- `fkv` 之外禁止 re-export 第三方类型；公共 API 只暴露 fkv 自有类型。

---

## 4. 技术选型

### 4.1 核心选型

| 领域 | 选型 | 理由 / 备注 |
|------|------|-------------|
| 语言 | Rust 2024 edition，nightly toolchain | edition 2024；fmt 需要 nightly |
| 异步运行时 | **compio 0.19.2**，features：`macros`、`fs`、`io`、`time`、`net`、`sync`、`process` | 不用 0.20 beta；单线程 Runtime + completion-based IO |
| 执行模型 | 1 shard = 1 OS thread = 1 compio Runtime | thread-per-core；Linux 绑核，macOS no-op |
| 任务 | `async fn` + `compio::spawn`，不引入 stackful coroutine | 取消 = drop future；RAII 释放 pin |
| 跨线程消息 | `crossfire` 3.x MPSC（async recv） | 请求 ingress / cloud 完成回传；waker 天然跨线程 |
| 完成通知 | `futures-channel::oneshot` | 每请求一个 completion；不回调到 shard 线程外 |
| 文件 IO | `compio-fs`：`read_at` / `write_at` / `sync_data` / `rename` / `read_dir` | O_DIRECT 用 `OpenOptions::custom_flags` 或 raw fd 兜底 |
| 缓冲 | 自研 `fkv_buf` 对齐 buffer + `compio-buf::{IoBuf, IoBufMut}` | 4 KiB 对齐；固定地址，禁止 realloc |
| 大 value 零拷贝 | `io-uring` READ_FIXED / WRITE_FIXED + `FixedBuffer`；managed read 直读用户 buffer | 注册在 compio runtime ring，CQ 与 runtime 合并，见风险 R1 |
| 哈希 | `gxhash::{HashMap, HashSet}` | AES / NEON 加速；`.cargo/config.toml` 按平台加 `+aes,+sse2` / `+aes,+neon` |
| 并发 map | `papaya` | 仅用于全局路由 / 跨 shard 只读表；shard 内不用 |
| 锁 | `parking_lot` | 仅 Store 生命周期、Attach / Detach；shard 热路径无锁 |
| 字符串 | `hipstr`（表名 / branch 名）；`bytes::Bytes`（key / value） | 减少分配；请求载荷零拷贝 |
| 时间 | `coarsetime`（单调时钟 / TTL）；`jiff`（格式化） | 热路径不用 `SystemTime` |
| 错误 | `thiserror`，扁平 `KvError`，`#[error(transparent)]` 转发 | 禁止 anyhow 进入库 |
| 日志 | `log` + `env_logger`；测试用 `ctor` + `log_init` | 禁止间隔线日志 |
| 编解码 | 手写 LE / varint + `zerocopy` 固定头 | 热路径不用 serde |
| 校验和 | `xxhash-rust` 的 `XXH3_64bits` | 与 eloqstore 完全一致 |
| 压缩 | `zstd`（libzstd），支持字典 | 与 eloqstore 对齐 |
| C++ 对照 | **Crubit**（pin commit）+ CMake；`_cpp` crate 只做差分验证 | 需要 nightly + `rustc-dev`；Crubit 不支持的签名收敛到 `cpp/shim` |
| 迁移账本 | **motedb 0.17** + `tree-sitter 0.25` + `tree-sitter-cpp 0.23` + `clap_args 0.1` | 只用于 `tool/fkv_ledger`；数据库 `.data/motedb.mote` gitignore |
| 验证环境 | `docker compose`：Ubuntu 26.04 + eloqstore 依赖 + RustFS | 功能验证 |
| 性能环境 | GitHub Actions `ubuntu-26.04` / `ubuntu-26.04-arm` | 只跑原生 Linux；共享 runner，取相对比值 |
| 兼容基线 | eloqstore `main` @ `82b774c62728b023b03c6e5e84e69416ffec9d74` | 以最新开发版为基线，每个里程碑记录一次 commit |
| S3 | `cyper` + `reqsign`（SigV4）+ `quick-xml` | 纯 compio 栈；可选 `aws-sdk-s3` feature 兜底 |
| Standby | `compio-process` 启 rsync / ssh | pidfd / 进程监管 |
| CLI | `clap_args` 宏 + `clap::Command` | 账本 / bench 工具 |
| 性能看板 | Vite 8 + Svelte 5 + Pug + `@1-/stylus`；复用 webc 样式与翻译流程 | `web/`，GitHub Pages |
| 测试 | `cargo-nextest`、`rstest`、`proptest`、`insta`、`criterion` | 集成测试在 `tests/` |
| 内存检查 | `miri`（codec / buf）、`loom`（跨线程队列）、`cargo-fuzz` | 24h fuzz 作为 1.0 门槛 |
| 静态检查 | `clippy -D warnings`、`cargo-deny`、`cargo-machete`、`cargo-udeps` | `./sh/clippy.sh` |
| 格式化 | nightly rustfmt + fkv `rustfmt.toml` | `tab_spaces = 2`、`imports_granularity = "Crate"` |

### 4.2 compio 依赖写法

```toml
[target.'cfg(target_os = "linux")'.dependencies]
compio = { version = "0.19", features = [
  "macros", "fs", "io", "time", "net", "sync", "process", "io-uring",
] }

[target.'cfg(not(target_os = "linux"))'.dependencies]
compio = { version = "0.19", default-features = false, features = [
  "macros", "fs", "io", "time", "net", "sync", "process", "polling", "runtime",
] }
```

- Linux 生产路径：io_uring。
- macOS 开发路径：polling driver；O_DIRECT / fixed buffer 能力用 `cfg` 探测，缺失时退化为 buffered IO。
- 所有 compio API 只出现在 `fkv_runtime` / `fkv_io_compio` / `fkv_object_s3` / `fkv_standby`，其余 crate 不直接依赖 compio。

### 4.3 关键 Rust 设计

1. **async/await 替代 stackful coroutine**
   - 每个请求 = 一个 future；`compio::spawn` 到 shard runtime。
   - 取消 = drop future；`PageRef` / `MappingSnapshot` / `FdRef` 全部 RAII，drop 即释放。
   - 禁止在任务 poll 栈内嵌套 `block_on`，禁止 inline 驱动调度器（compio thread-per-core 禁忌）。

2. **shard 内单线程状态**
   - `ShardInner` 使用 `Rc` / `Cell` / `RefCell`，不实现 `Send` / `Sync`。
   - 公共 API 边界只暴露 `Send` 的 handle / request。
   - `MappingTbl` 用 `Box<[Cell<u64>]>` 分块，读者 swizzle 时原地写，无原子操作。

3. **swizzle 不用裸指针**
   - `MappingValue` 为 8 字节 tagged word：`FilePage` / `Swizzled{slot, gen}` / `Free` / `Invalid`。
   - `MemCachedPage` 放在 shard 的槽位表；`slot + generation` 防悬垂；pin 计数阻止驱逐。
   - 避免 `NonNull<MemCachedPage>` 跨 await 的生命周期问题，也便于 Miri 检查。

4. **完成契约**
   - 每个请求持有一个 `oneshot::Sender<Result<Response>>`。
   - 任务完成时在 shard 线程调用 `complete`；async API await oneshot；sync API 在 runtime 外用轻量 `block_on`，禁止在 shard 线程调用。

5. **写序列化**
   - 每分区一个 FIFO 写队列 + 一个 async `WriteGate`；同一时刻只有一个写任务。
   - `max_write_concurrency` 用 `compio::sync::Semaphore` 实现背压；排队中的请求不占任务槽。
   - 后台 compaction / GC / archive 走同一队列，保证与用户写串行。

6. **IO 抽象**
   - `enum IoManager { Mem, Local, Cloud, Standby }` + `enum_dispatch`。
   - 不在热路径使用 `Box<dyn Trait>`；模式在 `Store::start` 时确定，运行期不变。
   - `Cloud` / `Standby` 是 `Local` 的 overlay，复用本地 manifest / page 语义。

7. **错误与 panic**
   - 非测试代码禁止 `unwrap` / `expect` / `panic!`；错误统一 `KvError`。
   - 库使用 unwind；发布 binary 可 `panic = "abort"`；任务边界不 `catch_unwind`（abort 语义下无意义）。
   - `debug_assert!` 只用于不变量，不作为错误处理。

8. **unsafe 策略**
   - 仅 `fkv_codec`、`fkv_buf`、`fkv_io_compio`、`fkv_large`（pinned）允许 `unsafe`。
   - 每个 `unsafe` 块必须有 `// SAFETY:`；crate 级 `#![deny(unsafe_op_in_unsafe_fn)]`。
   - `fkv_codec` / `fkv_buf` 跑 Miri；越界优化用 `get_unchecked` 前必须先有长度断言。

---

## 5. 关键风险与对策

| 编号 | 风险 | 影响 | 对策 |
|------|------|------|------|
| R1 | compio 0.19 未公开 io_uring fixed-buffer 注册 API | 已解决 | 通过 `Runtime::as_raw_fd` 取 runtime ring fd，syscall 注册 sparse buffer 表；自定义 `ReadFixed` / `WriteFixed` `OpCode` 走 `Runtime::submit`，完成事件进入 compio CQ；不支持时 pread/pwrite fallback |
| R2 | macOS polling driver ≠ io_uring | 本地开发测不出真实性能 | CI 双平台：Linux 跑 bench / crash；macOS 只跑功能测试 |
| R3 | S3 SigV4 / conditional PUT / List 分页细节多 | cloud 模式延期 | `fkv_object` trait 隔离；RustFS 集成测试；保留 `aws-sdk-s3` 可选 backend |
| R4 | swizzle 的 Rust 安全 / 性能权衡 | 点读热路径退化 | 8 字节 slot+gen；微基准对比 `Arc` 方案；`PageRef` 只在 shard 内 |
| R5 | async 取消导致 pin / snapshot 泄漏 | 内存泄漏 / 页无法回收 | 全部 RAII；shutdown / 随机取消测试；`Drop` 断言计数 |
| R6 | 盘面字节兼容成本 | 无法复用 eloqstore 工具 / 夹具 | M1 生成 C++ 夹具做交叉解析；兼容层限制在 `fkv_page` / `fkv_manifest` |
| R7 | 45 crate 编译时间 | 开发迭代慢 | workspace 依赖集中；dev 不开 LTO；`-Zthreads`；必要时合并小 crate |
| R8 | compio 0.20 beta API 变动 | 升级成本 | 锁 0.19；compio 只在 4 个 crate 出现 |
| R9 | O_DIRECT 对齐 / macOS 不支持 | 本地 IO 失败 | `fkv_buf` 统一 4 KiB 对齐；非 Linux 自动退化 buffered IO |
| R10 | 后台任务抢占前台 p99 | 尾延迟不达标 | 后台并发信号量 + 每 N page/segment 主动 yield；bench 验证 |

---

## 6. 里程碑

### M0 工程骨架（2–3 天）

- [ ] 创建 `fkv/` workspace：45 members、`resolver = "3"`、edition 2024。
- [ ] `rust-toolchain.toml`、`rustfmt.toml`（复用 fkv）、`clippy.toml`、`deny.toml`。
- [ ] `xtask`：依赖方向检查、`cargo clippy`、`cargo nextest`、bench 汇总。
- [ ] `fkv_types` / `fkv_error` / `fkv_codec` / `fkv_config` 首版；config INI 解析用 proptest。
- [ ] `fkv_testkit`：临时目录、对齐 buffer、确定性时钟。
- [ ] CI：Linux（fmt / clippy / test / bench smoke）+ macOS（test）。
- **出口**：`./sh/clippy.sh` 0 warning，`./test.sh` 全绿。

### M1 格式层（5–7 天）

- [ ] `fkv_page`：Data / Index / Overflow 编解码 + iterator + restart + checksum。
- [ ] `fkv_manifest`：snapshot / log / branch metadata / archive 命名。
- [ ] `fkv_compress`：zstd 字典 + 单值压缩。
- [ ] 锁定 `COMPAT_BASELINE`（eloqstore commit）；写 `fixtures/` 生成器。
- [ ] 用 eloqstore 生成 page / manifest 夹具，Rust 侧交叉解析；Rust 写出的夹具回喂 C++ 解析。
- [ ] `cargo-fuzz`：page / manifest 解码 24h 无 panic。
- **出口**：proptest 1e6 轮 round-trip；C++ ↔ Rust 夹具双向解析通过。

### M2 单 shard 本地引擎（10–15 天）

- [ ] `fkv_buf` / `fkv_mapping` / `fkv_cache` / `fkv_btree` / `fkv_root`。
- [ ] `fkv_io` + `fkv_io_mem` + `fkv_io_compio`。
- [ ] `fkv_recovery`：冷启动 replay、torn tail 截断。
- [ ] `fkv_task_read` / `scan` / `write` / `reopen`。
- [ ] `fkv_cli` 单 shard bench。
- [ ] crash harness：随机 kill -9，10k 轮。
- [ ] 字节兼容 L1：C++ 写 → Rust 读 → Rust 续写 → C++ 再读。
- **出口**：点读 / scan / 批量写 / 重启恢复全通过；L1 双向互读通过；单 shard 吞吐基线。

### M3 多 shard + 后台维护（10–15 天）

- [ ] `fkv_runtime` / `fkv_shard` / `fkv_api`：请求队列、admission、写队列、关停。
- [ ] `fkv_compaction` + `fkv_gc` + `fkv_task_compact/gc/archive`。
- [x] TTL tree、CleanExpired：TTL key 编码、TTL 索引写入、get / scan 过期过滤、按时间清理。
- [x] Truncate、Drop：全表释放 mapping / 双 root 归零 / manifest 重置；Drop 删除 manifest 与本地 / 远端 data 对象。
- [x] `fkv_metrics`：每 shard 请求计数 / 延迟直方图 / cache / file 指标 + `MeteredIo`。
- [x] 日志：`log` crate，runtime / shard 生命周期 info、请求 debug、错误 warn，key=value 结构化字段；日志捕获测试。
- [x] 并发 stress 首版：4 shard、4 writer + 3 reader 并发 put/get/scan，校验全量数据与指标 delta。
- **出口**：8 shard stress 24h 无泄漏 / 无 panic；p99 达标。

### M4 Cloud 模式（10–15 天）

- [x] `fkv_object` + `fkv_object_s3`（RustFS 1.0.4；CI 起真实 rustfs 跑集成测试）。
- [x] `fkv_cloud`：整文件上传 / 下载；本地文件 cache 按字节预算 LRU 淘汰；manifest 前先 flush dirty 文件。
- [x] term CAS：`CURRENT_TERM_<branch>_<pg_id>` 条件上传 + ExpiredTerm fencing；shard Attach 后本地校验写 term。
- [x] `fkv_task_reopen` auto-reopen：`ResourceMissing` 重试 + 可注入 Sleeper；CloudStore 异步 manifest 出现集成测试。
- [x] `fkv_task_prewarm`：manifest / data / segment 预热与统计；大 value 场景集成测试。
- **出口**：RustFS 集成测试、断网 / 重启 / 缓存驱逐 / 换主 fencing 全通过（已覆盖 RustFS 整文件上传下载 / conditional PUT / cache 重下载 / term fencing）。

### M5 大 value + Standby（8–12 天）

- [x] `fkv_large`：segment mapping、`SegmentIo` 段文件读写、metadata trailer、读写 / 释放计划；7 tests。
- [x] Store 级大 value put / get：segment 写入 / 读取、metadata trailer、manifest segment mapping snapshot + log 持久化。
- [x] segment GC：put_large 覆盖 / 删除时释放旧逻辑 id，`Store::gc` 回收空 segment 文件；Drop 删除 segment 文件。
- [x] runtime 大 value：`Request::PutLarge/GetLarge`、`ShardConfig`/`RuntimeConfig` segment 参数、`Runtime::put_large/get_large`，含 gc / reopen / TTL 端到端测试。
- [x] cloud 大 value：CloudStore 以整 segment 文件上传 / 下载，GC / Drop 同步删除远端 segment 对象。
- [x] managed buffer 读路径 / pinned 写路径：直接读入调用方 4 KiB 对齐 buffer，按 segment 从 pinned buffer 写出。
- [x] io_uring fixed buffer：Linux `CompioStore` 注册稀疏 buffer 表，`READ_FIXED` / `WRITE_FIXED` 读写 segment；fkv_large fixed 读写计划；非 Linux fallback。
- [x] fixed IO 接入 Store / runtime：`put_large_fixed` / `get_large_into`，Request `PutLarge/GetLarge` 使用 fixed buffer。
- [x] `fkv_standby`：StandbyMaster / Replica、远端 list / fetch、stage manifest（校验后原子安装）、rsync / ssh 骨架；真实 rsync + CompioStore 集成测试。
- **出口**：1–10 MiB value 读写正确；standby 切换后数据一致（已覆盖 replica 同步后 reopen 读取一致）。

### M6 API / 兼容 / Bench（5–8 天）

- [ ] `fkv_sdk`：`put/get/delete/scan` + typed traits，对齐 eloqstore Rust SDK。
- [ ] `fkv_cli` / `fkv_bench`：对齐 `simple_bench` 参数。
- [ ] 与 eloqstore 同机对比，输出 benchmark 报告。
- **出口**：API 示例全通过；性能差距在验收阈值内。

### M7 1.0 硬化（持续）

- [ ] fuzz、miri、loom、chaos（磁盘满 / 慢 IO / 云超时）。
- [ ] 文档：`doc/architecture/` 按子系统对齐 eloqstore。
- [ ] 无死代码：清理孤儿 `pub`、死 getter、未消费导出。
- [ ] 发布检查：semver、license、README、examples。

---

## 7. 验收标准

1. `./sh/clippy.sh` 0 warning（禁止 `allow` 绕过，按最佳实践改写）。
2. `./test.sh` 全绿（`cargo nextest run --all-features`）。
3. 非测试代码无 `unwrap` / `expect` / `panic!`；错误全部走 `KvError`。
4. 崩溃一致性：10k 随机 kill point 后 replay 不丢已确认写、不产生损坏页。
5. **字节兼容**：按 `./doc/migration.md` 的 V4 交叉读，C++ ↔ Rust 双向可读；同一 store 交替打开后全量 scan 一致。
6. 性能（同机 Linux NVMe，对比 eloqstore）：
   - 冷点读 p99 ≤ 1.2×；
   - 批量写吞吐 ≥ 0.85×；
   - scan 吞吐 ≥ 0.9×；
   - RSS ≤ 1.1×。
7. 云模式：RustFS 全部集成测试通过；term 过期后旧 writer 被 fence。
8. fuzz：page / manifest 解码 24h 无 panic。
9. `fkv_page` / `fkv_manifest` / `fkv_mapping` 行覆盖率 ≥ 90%，关键不变量有 proptest。
10. 公共 API 有 doc + example；`cargo doc` 无 warning。

---

## 8. 工程规范（来自 rust_review）

- 依赖用 `cargo add` 添加，禁止手改 `Cargo.toml` 依赖列表。
- 注释用中文；禁止雷同字符串，重复字面量提取常量。
- 多用迭代器 / `find` / `scan` / `try_for_each`；少用 `for i in 0..len`，避免无谓 `collect`。
- 能编译期算的不放到运行期；魔法数字用 `const`。
- 避免无谓 `clone` / `Arc`；只捕获必要字段，不捕获整个 `&self`。
- 错误定义在 `fkv_error`；依赖库错误用 `thiserror` 透明转发。
- 禁止 `dyn`；用泛型 / `impl Trait` / `enum_dispatch`。
- 日志用 `log`；测试用 `ctor` + `log_init`；禁止间隔线。
- 集成测试放 `tests/`；只测复杂逻辑、边界、关键路径。
- 完成后跑 `./sh/clippy.sh`，再跑 `./test.sh`；修复所有 warning，不用 `allow` 绕过。
- 禁止死代码；`pub` 孤儿接口定期清理，连带清理 re-export 与文档。

---

## 9. 已定决策与待澄清问题

**已定**：

| 编号 | 决策 | 结论 |
|------|------|------|
| D1 | 盘面格式 | **字节兼容**：只做迁移闭环；双向可读；不要求同 workload 逐字节一致 |
| D2 | 兼容基线 | 跟 `main` 最新 commit；`fkv_ledger sync` 一键 fetch + 更新 `COMPAT_BASELINE` + 重扫 |
| D3 | 互操作 | 单 writer 交替打开：C++ 写 → Rust 读 → Rust 续写 → C++ 读；不做双进程并发写；不要求跨语言崩溃恢复 |
| D4 | 验证环境 | `docker compose` + Ubuntu 26.04 + eloqstore 依赖 + RustFS 1.0.4；C++ oracle 单独 `oracle.yml` |
| D5 | `_cpp` 对照 | 44 个空壳已生成；真 C++ 对照限定在格式 / 算法层（约 11 个），runtime / io / tasks / cloud 用端到端 oracle；Crubit 只覆盖 `cpp/shim`，不支持的签名降级内部 C shim |
| D6 | 消息队列 | `crossfire` 3.x MPSC（async recv） |
| D7 | 迁移账本 | `tool/fkv_ledger`：**motedb** + tree-sitter + `clap_args`；数据库 `.data/motedb.mote` gitignore；scan 出 1415 unit（1368 unique，重载合并） |
| D8 | 目录 | `crate/<layer>/<crate>` + `crate/<layer>/<crate>_cpp`；`cpp/shim`、`tool/`、`docker/`、`fixtures/` 分拆 |
| D9 | crate 命名 | 统一 `fkv_` 前缀；facade 为 `fkv`；C++ 对照为 `<name>_cpp`；不做 Python SDK |
| D10 | 脚手架 | 89 个 package（44 Rust + 44 `_cpp` + `fkv_ledger`）已生成；`cargo check` / `clippy -D warnings` / `fmt --check` / `test.sh` 通过 |
| D11 | 1.0 范围 | Cloud / Standby / Branch / Archive / TTL / large value / zero-copy 全做 |
| D12 | CI / 性能 | GitHub Actions `ubuntu-26.04` / `ubuntu-26.04-arm`；action 用最新 major；性能只跑原生 Linux，独立 `fkv_bench` |
| D13 | 平台参数 | gxhash 无回退：`.cargo/config.toml` 自动加 x86_64 `+aes,+sse2`、aarch64 `+aes,+neon` |
| D14 | 迁移工具链 | 账本 CLI + `sync` + `map` + `next` + `evidence` + `edges`；工具规划见 `doc/migration-tooling.md` |
| D15 | 性能口径 | 绝对值 + 同 runner 相对比值都记录；相对比值做 CI 门槛 |
| D16 | 性能看板 | `web/`：Svelte + Pug + Stylus + webc 样式；`perf.yml` 合并历史并推 `gh-pages` |
| D17 | 分配器 | 账本使用 `rusty_alloc_default`，不再用 jemalloc / mimalloc |
| D18 | 基础 crate | `fkv_error` / `fkv_types` / `fkv_codec` / `fkv_config` 首版 |
| D19 | 端到端首版 | 格式 / 核心 / IO / read / scan / write / reopen / GC helper / `MemStore` 端到端 put-get-scan-delete；72 tests 通过 |
| D20 | COW 写路径 | `fkv_task_write` 改为按页 COW：只重写受影响 data page + 边界页，索引层重建；测试断言 COW 写次数远小于整树写 |
| D21 | 持久化 Store | `fkv_store` 泛型化，`CompioStore` 已跑通 put / get / scan |
| D22 | Manifest 持久化 | 首次写 snapshot，后续追加 log；`reopen` 恢复 mapping / root；MemStore + CompioStore 重开测试通过 |
| D23 | GC / Compaction | `PageMapper` 记录 garbage；COW 后释放旧逻辑页；`fkv_task_gc::gc_files` 删除空文件；`fkv_task_compact::compact_file` 搬 live page；`Store::gc/compact_file` 接 manifest |
| D24 | Manifest rotation | `manifest_limit` 超限时写新 snapshot 并 `switch_manifest` 原子替换；MemStore / CompioStore 测试 |
| D25 | Index 路径 COW | `fkv_task_write` 只重写 root→leaf 路径上的 index page；不再整层重建 |
| D26 | 批量 upsert 合并 | batch 按 data page 分组，同页多条一次 COW；跨页逐组处理 |
| D27 | Shard Runtime | `fkv_runtime` / `fkv_shard`：1 shard = 1 OS 线程 + 1 compio Runtime + crossfire 请求队列；`fkv_store` 接 `CompioStore` |
| D28 | Index rebalance | 路径节点低于 10% 填充时触发整层 index rebalance：按 data page 重新打包 index 页并释放旧页 |
| D29 | Cloud 模式 | `fkv_object` trait + `MemObjectStore`；`fkv_cloud` 本地 cache overlay；`fkv_object_s3` 用 object_store/aws + 独立 tokio worker |
| D30 | Term CAS | `CURRENT_TERM_<branch>_<pg_id>` 条件写；`CloudStore::upsert_term` 读 ETag + If-Match / If-None-Match，冲突重试 10 次；`Store::put_with_term` 返回 ExpiredTerm |
| D31 | AttachShard | `fkv_shard` 持有 branch / shard_id / term；未 attach 的读写返回 NotRunning；Attach 拒绝更旧 term，Detach 保留 term 防回退；`fkv_runtime` 新增 attach / detach / put_with_term |
| D32 | File-level cache | `fkv_io::FileIo` 增加整文件读写 / 大小；`fkv_cloud` 写页只落本地，manifest 前整文件上传；miss 时整文件下载；clean 文件按字节预算 LRU 淘汰，dirty 不淘汰；写已淘汰文件先恢复 |
| D33 | S3 + RustFS | `fkv_object_s3` 修 shutdown 命令；`docker/` 增加 rustfs-init（aws-cli 建 bucket）；`fkv_cloud` 集成测试覆盖整文件 PUT/GET、cache 重下载、If-None-Match / If-Match、term CAS；无 `EKV_S3_ENDPOINT` 时跳过；CI 双架构跑真实 RustFS |
| D34 | Standby | `fkv_standby`：Master / Replica 角色、`RemoteSource`、`LocalDirSource`、`RsyncShell`（本机 / ssh）、`RsyncSource`；data 先落 staging 再 rename，manifest 最后校验完整 record 链后原子安装；compio-process 0.10 beta 依赖新 compio runtime，先用 `std::process` + `CommandRunner`，后续替换；95 tests |
| D35 | fkv_large | `SegmentLayout`（segment id → file / offset）；large-value content `word0|K×id|[u16 len|metadata]` 与 eloqstore 对齐；`SegmentIo` 在 MemStore / CompioStore 落地（tail 补零、固定槽位）；`write_value` / `read_value` / `free_value` + mapping 序列化恢复测试 |
| D36 | TTL tree | TTL key = `expire_ts(8B BE) | user_key`；与主树共用 mapping，manifest 的 ttl_root / snapshot / log 同步持久化；`get_at` / `scan_at` 过滤过期值；`clean_expired` 按时间清理 TTL 条目并仅删除 expire_ts 匹配的主树值；stale TTL / 删除 / reopen 测试；106 tests |
| D37 | Truncate / Drop | `Store::truncate` 释放全部 mapping、root/ttl_root=MAX、`switch_manifest` 空快照，data 文件留给 GC；`Store::drop_table` 先删 manifest，再删本地 / 云 data 对象，清空内存；`Request::Truncate/Drop` + runtime 接口；Mem / Compio / Cloud / runtime 测试；111 tests |
| D38 | fkv_metrics | `ShardMetrics` 原子计数 + 17 桶延迟直方图（count/sum/min/max）；`MeteredIo` 统计 page / manifest / segment / file 字节；CloudStore 上报 cache hit / miss / evict / bytes；`Runtime::metrics()` / `shard_metrics()`；116 tests |
| D39 | Store large value | `Store::put_large` 按阈值调用 `fkv_large::write_value`，`get_large` 解码并读 segment；`BatchEntry` 增加 large flag / metadata，COW 合并保留 0b11；manifest snapshot 写 segment mapping，log 写带 u32 长度的 delta；truncate 同步释放；Mem / Compio / log replay / COW 测试；120 tests |
| D40 | Segment GC | `put_large` 先记录旧大 value，成功写入后 `fkv_large::free_value` 释放旧 segment id 并追加 manifest delta；`FileIo` 增加 segment 文件 list / remove；`gc_segment_files` 按 retained segment mapping 删除空文件；`Store::gc` 同时回收 data / segment；121 tests |
| D41 | 日志 | `fkv_runtime` / `fkv_shard` 使用 `log`：runtime/shard start/stop、attach/detach 为 info；请求成功 debug、失败 warn，字段 `shard_id/op/status/error/latency_us`；测试用内存 logger 校验结构化输出；122 tests |
| D42 | 并发 stress | 4 shard / 8 table / 4 writer / 3 reader：并发 batch put + get + prefix scan；join 后全量校验 base 与 800 个 writer key；metrics Put/Get/Scan delta 精确匹配请求数、无错误、延迟 count == total、每个 shard 均有流量与 page 写；连续 10 轮稳定；123 tests |
| D43 | Runtime large value | `fkv_api` 增加 `PutLarge` / `GetLarge` / `LargeValueReply`；shard 按 `segment_size` / `segments_per_file_shift` 配置 Store 与 CompioStore，term 校验后调用 put_large / get_large；runtime 暴露 `put_large` / `get_large`，指标归入 Put/Get；端到端覆盖 metadata、segment 文件、GC、TTL、重启 reopen；124 tests |
| D44 | Cloud large value | `CloudStore` 增加 segment 配置与 `SegmentIo`：写 segment 后组装整文件上传 `{tbl}/segment/{file_id}`，读 miss 时整文件下载并回填本地槽位；`FileIo` segment list / remove 联合本地与远端；Store 大 value 可在 Cloud 模式使用；125 tests |
| D45 | Auto-reopen / Prewarm | `AutoReopenOptions` + `Sleeper`：仅 `ResourceMissing` 重试，其它错误立即返回，默认 `ThreadSleeper`；CloudStore 远程 manifest 延迟出现测试。`fkv_task_prewarm` 顺序预热 manifest / data / segment，返回 `PrewarmStats`，CloudStore 增加 remote list / prewarm 原语；131 tests |
| D46 | Managed / pinned buffer | `fkv_large` 增加 `ManagedBuffer` / `PinnedBuffer` trait（`AlignedBuf`、slice）、`read_value_into` 逐 segment 直读调用方 buffer、`read_value_pinned` / `write_value_pinned` 4 KiB 对齐校验；Mem / Compio 测试；133 tests |
| D47 | io_uring fixed buffer | `fkv_io::FixedBuffer` + `FixedSegmentIo`；`CompioStore` Linux 独立 io_uring，sparse 64 buffer 表 + `register_buffers_update`，`READ_FIXED` / `WRITE_FIXED` 直读直写，非 Linux fallback；`fkv_large::read_value_fixed` / `write_value_fixed`；Linux 容器实测 + clippy；135 tests（macOS），Linux fkv_large 11 tests |
| D48 | Fixed IO 接入 Store/runtime | `Store::put_large_fixed` / `get_large_into` 使用 `FixedBuffer`；shard 启动时按 `fixed_buffer_size` 注册固定 buffer，`PutLarge/GetLarge` 优先 fixed IO、放不下自动 fallback；`MeteredIo` 透传 fixed IO 并统计；Store / runtime 端到端测试，Linux 容器实测（registered=true）；136 tests |
| D49 | Branch / Archive | `Store` 支持 active branch/term 与 file range 元数据；manifest snapshot/log 写分支记录；`create_archive` 生成 `manifest_<branch>_<term>_<tag>` 并按 tag 倒序保留 N 份；`create_branch` / `delete_branch` 生成/删除 branch manifest；`ManifestIo` 增加非活跃 manifest 读写/list/remove（Mem/Compio/Cloud）；139 tests |
| D50 | Branch/Archive API | `fkv_api` 增加 `CreateArchive` / `CreateBranch` / `DeleteBranch` 与 reply；shard attach 时 `Store::set_term`，请求映射 Store 方法；runtime 暴露 `create_archive` / `create_branch` / `delete_branch`；metrics 新增 3 个 Op；端到端覆盖命名/裁剪/错误/指标；140 tests |
| D51 | ArchiveCrond | `RuntimeConfig` 增加 `archive_interval_secs` / `archive_num_retained`；runtime 后台线程按 interval 为 attached shard 的已知 table 发送 `CreateArchive`，tag 为 20 位毫秒；detach 后停止，shutdown / Drop 唤醒并 join；集成测试覆盖生成、保留 2 份、detach 停止；141 tests |
| D52 | Branch 写路径 | `Store::open_branch` 从 `manifest_<branch>_0` replay 独立 mapping/root/segment mapping；branch 模式 put 写 data/segment 后整体重写 branch snapshot；支持继承父数据、branch 覆盖、重启持久化、大 value 继承/写入；main 与 branch 隔离；Mem / Compio 测试；143 tests |
| D53 | Runtime branch attach | `Store::prepare_attach` 清空 table 状态并切换 main/branch manifest 模式；AttachShard 设置 branch/term；首次访问 table 时 main 走 `reopen`、branch 走 `open_branch` 懒加载；测试覆盖 branch 继承/覆盖/返回 main 隔离/重新 attach 持久化；144 tests |
| D54 | Branch 并发隔离 | `PageMapper::reserve_until`；create_branch 按 1024 file id 块预留 data + segment 区间，父 branch 跳过并把新 max 持久化；branch snapshot 记录区间起点，open_branch 派生并校验 data/segment 上限；main/branch 两线程并发写 + 大 value，断言区间不重叠、数据互相不可见、重启区间保持；145 tests |
| D55 | Branch-aware GC | `manifest_retained_file_page_set` 汇总 active + 所有 branch/archive manifest 的 data/segment mapping，再并上内存 mapping；`gc_files_with_retained` / `gc_segment_files_with_retained` 按并集删除；测试 branch 文件在父 GC 后仍可读，delete_branch 后 branch-only 文件被回收；146 tests |
| D56 | Runtime auto-reopen | `RuntimeConfig`/`ShardConfig` 增加 retry / pending；Shard Reopen 使用 `Store::auto_reopen_with_sleeper` + `CompioSleeper`（compio timer，不阻塞线程）；branch 模式同样生效；测试覆盖延迟 manifest 重试成功与重试耗尽；148 tests |
| D57 | 长时间 stress | 多轮 start/attach → 4 writer + 2 reader 并发 → reopen 全量校验 → shutdown；累计 key 数校验；Linux 线程数回到基线、fd 增长上限；默认 4 轮测试 + `#[ignore]` 长稳（`FKV_STRESS_ROUNDS`，默认 100）；149 tests + 1 ignored |
| D58 | API 兼容首版 | Floor（<= key 的最大 entry，含前页回退 / 大 value / NotFound）；Truncate position（删除 >= position，空串整表）；`Store::read` / `Runtime::read` 缺失返回 NotFound；`Request::Floor`、`Truncate{position}`、`Runtime::floor/read/truncate_from`；Mem/Compio/runtime 兼容测试；154 tests |
| D59 | Scan inclusive | `ScanOptions { begin_inclusive, end_inclusive }`；`scan_entries_with` 对齐 C++：seek begin、exclusive 跳过相等项、end 空上界、inclusive 比 <=；Store `scan_opts` / `scan_with_options`，`Request::Scan` 增加两布尔，Runtime `scan_opts`；Store/Runtime 兼容测试；155 tests |
| D60 | Scan 分页 | `PageOptions { page_entries, page_size }` + `ScanPage { entry_li, has_remaining }`；entry/字节双限制，首项超限仍返回，正好取完 remaining=false；Store `scan_page`，`Request::Scan` 增加分页字段，`ScanReply`，Runtime `scan_page(ScanPageOptions)`；Store/Runtime 分页兼容测试；156 tests |
| D61 | Scan 大 value 拒绝 | `ScanOptions.reject_large_value` 默认 true；命中 `is_large_value` 返回 `LargeValueUnsupported`，顺序对齐 ScanTask（entry 限制先于大 value 检查、大 value 检查先于字节限制）；Store 分页按 task page 批量推进，超过当前页限制的大 value 不提前报错；truncate 内部扫描允许大 value；Store/Runtime 测试；158 tests |
| D62 | Bench 看板 | `fkv_bench` 新增端到端 `put_kv_s` / `put_batch_s` / `get_kv_s` / `scan_kv_s` / `scan_page_s`，支持 `--impl/--rows/--value-size/--batch-size`，JSON history 保持兼容；`perf.yml` 改为 main push + 手动/每周，x86/arm 跑 bench、校验 JSON、合并 gh-pages 历史并部署；web 构建验证；159 tests |
| D63 | Branch/term 文件命名 | `PageIo::set_file_identity` 默认 no-op；Store 在 with_branch / prepare_attach / set_term / open_branch 传播 branch/term；CompioStore 用 `data_<id>_<branch>_<term>` / `segment_<id>_<branch>_<term>` 新建文件，读写/删除按 file id 在目录中定位（跨 term 可读）；MeteredIo / CloudStore 透传；IO + Store 命名测试；161 tests |
| D64 | 文件路径缓存 | CompioStore 增加 data/segment `(TableIdent, FileId) -> PathBuf` 缓存；首次 lookup 扫描目录并缓存，新建文件直接按 identity 构造并缓存；读/写/删除/list 使用缓存，remove 与 `set_file_identity` 失效；`path_scan_num` 计数；缓存命中/失效测试；163 tests |
| D65 | SDK / CLI / facade | `fkv_sdk::KvStore` 封装 `Store<CompioStore>`：open/reopen、put/put_kv/get/read/delete、scan/scan_opts/scan_page/floor、truncate/truncate_from/drop_table、put_large/get_large，`KvApi` trait，`now_ts` 毫秒墙钟；`fkv_cli` 提供 `fkv put/get/del/scan/floor/truncate`（`name.partition` 表名、可选 INI 配置）；`fkv` facade 收敛公开面；SDK/CLI 端到端测试；163 tests |
| D66 | Release / Perf / Docker 工具链 | `release.yml` 在 Blacksmith x64/arm 构建并发布滚动 `perf-latest`（含 `commit.txt`）；`perf.yml` 由 release 完成触发，下载预编译 bench 跑 x86/arm，经 GitHub contents API 合并 gh-pages `perf.json`；`docker-test.sh` 自动安装 cargo-zigbuild/zig/Docker（OrbStack），交叉编译测试二进制后在 `debian:bookworm-slim` + io_uring 权限下执行；`Dockerfile.test-run` |
| D67 | 目录索引预加载 / 变更检测 | CompioStore 由 `(table,file_id)` 分散缓存改为每分区目录一份 `DirIndex`（data+segment 一次 `read_dir` 建齐），记录目录 mtime+len 签名；负查询仅在签名变化时重扫；读取命中已删/被替换文件时 `forget_path` + 重扫一次；`PageIo::preload_paths` / `invalidate_path_cache` 默认 no-op，CompioStore 实现并透传 MeteredIo / CloudStore / Store / SDK；新增预加载、外部新增文件、stale path 恢复测试；169 tests |
| D68 | Scan 过期项语义 | 核对 C++ baseline：ScanTask / ReadTask / Floor 均不判断 `expire_ts`，过期项只由 CleanExpired 物理删除；fkv 默认保留读时过滤，新增 `Store::with_ttl_read_filter(false)` 切换为 oracle 的惰性清理语义；分页时过期项不占 entry/byte 配额且游标跨物理页推进；新增 oracle 语义与 TTL 分页测试；171 tests |
| D69 | 写时 TTL / TriggerTTL | `prepare_write_batch`：`expire_ts <= now` 的 upsert 不落主树，只生成 TTL 索引删除；过期 delete 仍删数据；`Store::put_at` 注入时间；`ttl_next_map` 跟踪最早到期时间，写后到期即调用 `clean_expired`（对齐 C++ TriggerTTL），reopen/truncate/drop 重置；新增 task_write 单测、写时丢弃、写后触发、全过期重开测试；178 tests |
| D70 | fixed IO 异步 completion | `FixedBuffer` 改为 `Arc<AlignedBuf>` 共享，供 in-flight op 持生命周期；`Runtime::as_raw_fd` 取 compio ring fd，`io_uring_register` 注册 sparse 64-slot buffer 表；`FixedRead` / `FixedWrite` 自定义 `compio::driver::OpCode`，经 `Runtime::submit` 进入同一 completion queue；删除独立 io_uring + 50µs 轮询；不支持 fixed opcode 时 `pread`/`pwrite` 线程池回退；`FixedBuffers` drop 时注销 buffer 表；新增 Linux 专用 fixed 测试（多 buffer、偏移、普通 IO 交错、blocking fallback）；178 tests + 3 Linux-only |

**仍待澄清**（完整清单见 [`questions.md`](./questions.md)）：

- Q1：账本下一步先做 `verify` 还是 `scaffold`。
- Q5：`_cpp` 验证逐字节还是结构等价；哪些 crate 进 PR。
- Q6：`fkv_bench` 端到端 workload 是否复刻 `simple_bench`。
- Q7：`fkv_bench` 输出 JSON / `compare.yml` 的格式。

---

## 10. 下一步

1. 确认 `questions.md` 的 Q1 / Q5–Q6。
2. ~~创建 workspace 与 89 个 package~~（已完成，位于 `./`）。
3. ~~迁移账本脚手架~~（已完成：motedb + tree-sitter，scan 出 1415 unit / 1368 unique）。
4. ~~`fkv_error` / `fkv_types` / `fkv_codec` / `fkv_config` 首版~~（已完成）。
5. ~~端到端首版~~（已完成，62 tests）：`fkv_store` 内存模式 put / get / scan / delete 全通。
6. ~~按页 COW（替换整树重写）~~（已完成，64 tests）。
7. ~~`fkv_io_compio` 接 `fkv_store`~~（已完成）。
8. ~~manifest 持久化 + reopen~~（已完成）。
9. ~~compaction / file GC 执行~~（已完成）。
10. ~~manifest rotation~~（已完成）。
11. ~~index 路径 COW~~（已完成）。
12. ~~批量 upsert 合并~~（已完成）。
13. ~~shard runtime / 请求队列~~（已完成）。
14. ~~index 节点合并 / rebalance~~（已完成）。
15. ~~Cloud 模式首版~~（已完成）。
16. ~~Term CAS~~（已完成）。
17. ~~AttachShard / DetachShard 接入 runtime / shard~~（已完成）。
18. ~~file-level cloud cache~~（已完成）。
19. ~~S3 + RustFS 集成测试~~（已完成；CI 双架构）。
20. ~~`fkv_standby` 首版~~（已完成）。
21. ~~`fkv_large` 首版~~（已完成，segment + metadata trailer）。
22. ~~TTL tree / CleanExpired 首版~~（已完成）。
23. ~~Truncate / Drop 首版~~（已完成）。
24. ~~`fkv_metrics` 首版~~（已完成）。
25. ~~Store 级大 value put / get~~（已完成）。
26. ~~segment GC~~（已完成）。
27. ~~日志首版~~（已完成）。
28. ~~并发 stress 首版~~（已完成）。
29. ~~runtime 大 value 请求~~（已完成）。
30. ~~cloud 大 value~~（已完成）。
31. ~~auto-reopen / prewarm 首版~~（已完成）。
32. ~~zero-copy managed / pinned buffer 骨架~~（已完成）。
33. ~~io_uring fixed buffer 真零拷贝~~（已完成）。
34. ~~fixed IO 接入 Store / runtime~~（已完成）。
35. ~~Branch / Archive 首版~~（已完成）。
36. ~~Branch / Archive runtime 接入~~（已完成）。
37. ~~ArchiveCrond 首版~~（已完成）。
38. ~~Branch 独立写路径~~（已完成）。
39. ~~Runtime branch attach~~（已完成）。
40. ~~Branch 并发写隔离~~（已完成）。
41. ~~Branch-aware GC~~（已完成）。
42. ~~auto-reopen 接入 runtime / compio timer~~（已完成）。
43. ~~多轮长时间 stress 测试~~（已完成）。
44. ~~API 兼容首版~~（已完成）。
45. ~~Scan begin/end inclusive~~（已完成）。
46. ~~Scan 分页 / HasRemaining~~（已完成）。
47. ~~Scan 大 value 拒绝~~（已完成）。
48. ~~Bench 看板首版~~（已完成）。
49. ~~Branch/term 感知文件命名~~（已完成）。
50. ~~CompioStore 文件路径缓存~~（已完成，163 tests）。
51. ~~SDK / CLI / facade~~（已完成）。
52. ~~Release / Perf / Docker 工具链~~（已完成）。
53. ~~目录索引预加载 / 目录变更检测~~（已完成，169 tests）。
54. ~~Scan 过期项语义~~（已完成：默认读时过滤，可切 C++ 惰性语义，171 tests）。
55. ~~写时 TTL / TriggerTTL~~（已完成，178 tests）。
56. ~~fixed IO 异步 completion~~（已完成：compio runtime ring + 自定义 OpCode，178 tests + 3 Linux-only）。
57. 下一步：真实 24h 稳定性运行、C++ 性能数据接入看板、压缩写路径。
7. 工具链规划：`doc/migration-tooling.md`。
8. 开子代理按 `rust_review` 审查每一层，clippy / test 作为硬门槛。
