# fkv

Rust 版 eloqstore。设计见 `./todo.md`，待澄清问题见 `./questions.md`。

- `crate/`：44 个 Rust crate + 44 个 `_cpp` 对照 crate，统一 `fkv_` 前缀。
- `cpp/shim`：给 Crubit 的简单类型 C++ 包装。
- `tool/fkv_ledger`：motedb + tree-sitter 迁移账本。
- `doc/migration.md`：迁移方法、`_cpp` 对照矩阵、账本数据模型。
- `doc/migration-tooling.md`：程序化迁移流水线、工具清单、指标。
- `docker/`：docker compose 验证环境。
- `web/`：GitHub Pages 性能看板（Svelte + Pug + Stylus + webc 样式/翻译流程）。
- `.data/`、`fixtures/generated/` 不入库。

基线：`COMPAT_BASELINE`。

## 测试与性能流水线

- 本地 Linux 测试（交叉编译后进容器运行，不需要在容器里编译）：
  ```bash
  cargo install cargo-zigbuild
  ./docker-test.sh                 # 全部 crate
  ./docker-test.sh -p fkv_store    # 指定 crate
  ```
  zig / python3 / docker / cargo-zigbuild 缺失时脚本会自动用 mise / brew / cargo 安装；
  docker 缺失时通过 `brew install --cask orbstack` + `orb start` 启动。
- `release.yml`：main 变更后构建 `fkv_bench` x86_64 / aarch64，上传滚动的 `perf-latest` release。
- `perf.yml`：release 完成后触发，不 checkout 仓库；下载预编译二进制跑 benchmark，
  合并历史后直接更新 gh-pages 的 `perf.json`，不重建站点。
- 需要安装 Blacksmith GitHub App（`blacksmith-sh`）并让仓库可用对应 runner label；
  Actions 运行本身不需要 Blacksmith API key。NVMe 路径可用仓库变量
  `PERF_BENCH_ROOT` 指定；Blacksmith 文档不保证根盘为 NVMe。
