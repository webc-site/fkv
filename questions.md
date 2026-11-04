# fkv 剩余问题

已定：

- 迁移账本纯 CLI；class + method + function 粒度；`ignored` 可标记。
- `sync` 一键更新 `COMPAT_BASELINE` 到 `main` 最新 commit，并重扫。
- 性能同时记录绝对值和同 runner 相对比值；相对比值做门槛。
- C++ oracle 单独 GitHub Actions，不阻塞 PR。
- 只做 S3；不做 Python SDK / 公开 C ABI；Apache-2.0。

## Q1 账本后续功能

- 已实现：`sync / baseline / scan / map / next / list / claim / status / ignore / evidence / edges / stats`。
- 待补：`scaffold`（生成 Rust 骨架）、`fixture`（调 oracle 生成夹具）、`verify`（跑差分并写 evidence）、`report`（覆盖率/进度）、`changed`（source_hash 变化）。
- **需要你定**：先做 `scaffold` 还是 `verify`？建议先 `verify`，因为格式层已经开始写代码。

## Q5 `_cpp` 验证强度

- 格式 crate 差分：逐字节相等，还是结构等价？
- 每次 PR 跑哪些：先 `fkv_page` / `fkv_manifest` / `fkv_mapping`，还是一次全上？
- **建议**：固定布局逐字节；zstd / 文件 ID 结构等价；PR 先三个格式 crate。

## Q6 `fkv_bench` 端到端 workload

- 当前只有 gxhash 微基准。
- 端到端是否直接复刻 eloqstore `simple_bench` 参数：
  `--workload=write-read --kv_size=1024 --batch_size=20000 --max_key=10000000 --read_per_part=4 --partitions=1`？
- **建议**：先 Rust-only 基线，再接 C++ oracle 同 runner 相对比值。

