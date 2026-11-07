# 03 IO 栈

## trait 划分（`fkv_io`）

| trait | 职责 |
|---|---|
| `PageIo` | 固定页读写：`read_page` / `write_page` / `read_page_li` / `write_page_li` / `sync_data` / path 缓存 / `set_file_identity` |
| `ManifestIo` | manifest：`append_manifest` / `switch_manifest` / `reset_manifest` / 具名 manifest 的读写与列举（branch / archive） |
| `SegmentIo` | 大 value segment 页读写 |
| `FixedSegmentIo` | 注册内存上的零拷贝 segment 读写 |
| `FileIo` | 整文件读写 / 大小 / list / remove（云端整文件 cache 与上传） |
| `TermIo` | `read_term` / `upsert_term`（term CAS） |

引擎只依赖 trait；`fkv_store` 是唯一把具体后端装进 `Store<I>` 的地方。

## 后端

| 后端 | 场景 |
|---|---|
| `fkv_io_mem` | 单测 / 内存模式 |
| `fkv_io_compio` | 本地文件：compio-fs（Linux io_uring，macOS polling） |
| `fkv_io_sim` | 确定性模拟：延迟 / 故障 / 重排 / `crash` / `restart` / 操作日志 |
| `fkv_cloud` | 在本地后端上叠加对象存储（本地 cache + LRU / 预算 + S3 overlay） |
| `fkv_object_s3` | S3 / RustFS |
| `fkv_standby` | rsync / ssh 拉取 |

## `fkv_io_compio` 现状

- **FD 缓存**：按 `(path, 读/写)` 复用已打开 `File`，O(1) LRU；上限来自
  `fd_limit`（`KvOption::fd_limit` 按 shard 均分）。只读命中额外用 `(dev, ino)`
  校验路径未被外部替换（standby/rsync 用 rename 换文件）。
- **缓冲池**：`read_page` / `write_page` 复用按大小分组的缓冲，避免每 IO 分配；
  读回长度与页大小不符走短读 / EOF 语义（见下）。
- **O_DIRECT**：Linux 上 data / segment 文件可按需 `O_DIRECT`，配 4 KiB 对齐
  缓冲池（全局字节上限）；非 Linux 或页大小不满足对齐时保持 buffered。
  **不支持 O_DIRECT 的文件系统**（tmpfs / 部分 overlay / 网络文件系统）上，
  open 返回 `EINVAL` / `EOPNOTSUPP` 时按路径类别（data / segment 独立）一次性
  回退 buffered 并打 `warn` 日志（`direct_fallback_num` 可诊断，io-18）。这是
  对 C++（硬编码 `O_DIRECT`、无回退，同一路径直接 `IoFail`）的有意偏离：升级
  前 fkv 默认 buffered，回退避免 tmpfs / 容器 emptyDir 上首次写即失败。已有
  同类 direct 句柄时不关闭开关（只让本次 open buffered），避免对齐 fd 配非
  对齐缓冲再次 `EINVAL`。open 成功返回后会复查类别开关（io-21）：若 await
  期间另一条失败 open 已关闭开关，则丢弃该 direct 句柄、不置位「已打开」，
  按 buffered 重开——保证「开关关闭」与「缓存里没有 O_DIRECT 句柄」一致。
- **固定写槽位（Linux）**：`io_uring_register` 注册 16 个写槽位，合批写优先走
  `WRITE_FIXED`；槽位缓存上限独立于 `max_inflight_write`，超限回落普通写池。
- **写批与在飞上限**：`write_page_li` 按 `max_write_batch_pages` 合并连续页，
  提交窗口由 `max_inflight_write` 限制；两者都来自 `KvOption` / `fkv_config`。
- **checksum 与错误语义**：读页后按 `verify_checksum` 校验；短写返回 `IoFail`；
  0 字节读是确定性 EOF（可回退旧 term 候选），非零短读重试耗尽后按 `Corrupted`
  上报；segment 短读按定长段零填充。
- **`sync_data(tbl)`**：只 fdatasync 本表写过的 data / segment 文件（脏集合），
  不误打 manifest。
- **manifest 持久化**：`switch_manifest` 写临时文件 → sync → rename → fsync
  父目录；`append_manifest` 首次创建文件或新建分区目录后同样 fsync 目录。
- **目录操作**：`read_dir` / `metadata` / `create_dir_all` 走 `spawn_blocking`，
  不阻塞 reactor；目录存在性与目录项签名有缓存。
- **managed read（Linux）**：定长 segment 读可走 compio provided-buffer
  （`read_managed_at`），尺寸不合适时回落自管池。该路径与 O_DIRECT **互斥**
  （io-16）：O_DIRECT 段读要求缓冲地址 / 长度页对齐，而 runtime 缓冲池的
  默认分配器不保证地址对齐；`with_direct_io(true)` 在 `segment_size` 页对齐
  （`direct_segment()`）时会显式关闭 managed 开关并告警，仅 data 页 direct、
  segment 仍 buffered 时 managed 仍可用。生产装配（shard / cloud local
  cache）默认 O_DIRECT，故 managed 只在 buffered segment 配置下生效。fixed
  注册读走 `READ_FIXED` + 注册 `AlignedBuf`，与本池无关。

## 路径与命名

- 分区目录：`{root}/{tbl_name}.{partition_id}/`
- data 文件：`data_<file_id>_<branch>_<term>`（新建时按当前 branch/term；
  读取按 file id 在目录内匹配，跨 term 可读）
- segment 文件：`segment_<file_id>_<branch>_<term>`
- manifest：本地 main 固定 `manifest_main_0`（sto-70）；Cloud active 与 archive
  为 `manifest_<branch>_<term>` / `manifest_<branch>_<term>_<tag>`；branch 活跃
  名带 attach term：Cloud 为 `manifest_<branch>_<attach term>`（sem-60 写时晋升，
  `CreateBranch` 初始产物与回退链成员仍为 `_0`），Local 恒
  `manifest_<branch>_0`
- term：`CURRENT_TERM_<branch>_<pgid>`

### term 语义

- attach 时 `claim_term` 落 term 文件，Local / Mem / Cloud / Sim 后端一致；Local
  也写该文件属于有意保留（C++ Local 不写），否则 `Request::Put { term }` 的
  fencing 契约会随后端分叉。同 term 幂等，低 term 返回 `ExpiredTerm`。
- detach 归零运行期 `term` / `branch`（对齐 C++ `slot.term_.store(0)`），但**不删**
  term 文件：旧 term 的重新 attach 由持久化 CAS 拒绝，不依赖内存状态。
- term 变化后 reopen 把 allocator 跳到下一文件边界（`install_replay_state`），
  旧 term 的数据文件仍按 file id 可读。

## 分层与云端

`fkv_cloud` 不直接依赖 compio 后端，而是包一层 `Store<CloudStore<Local, Remote>>`：
本地仍是 `CompioStore`，`FileIo` 提供整文件读写用于上传 / 下载；远端实现
`ObjectStore`（S3）。写侧支持 `put_range` 增量上传（远端正向支持时按区间写，
不支持时回落整对象），读侧按 manifest 映射出的 `(branch, term)` 定位对象。
term CAS 已实现（单次 GET 同时取 value 与 etag，失败重试有上限）。

## 已知缺口

- 短读重试耗尽后可能回退旧 term 候选。
- GC 按 `(branch, term)` 变体精确判删，同 file id 跨 branch 不误留。
- data 页常驻缓存不做：data 侧依赖内核 page cache 或 direct IO。

完整清单见 [`06-gaps.md`](./06-gaps.md)。
