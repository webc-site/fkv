# 01 P0：没有真正的 C++ oracle / 交叉读写验证

## 现象

- `crate/**/*_cpp` 共 44 个，全部只有一行占位注释，例如：
  - `crate/format/fkv_page_cpp/src/lib.rs`
  - `crate/engine/fkv_store_cpp/src/lib.rs`
- `cpp/eloqstore` 为空目录
- `cpp/shim/README.md` 明确写“真正的 shim 逐个补齐”
- `COMPAT_BASELINE` 已存在，但只被 `fkv_bench` 用来读取 baseline 元信息
- `oracle.yml` 只 build C++，没有跑 C++/Rust 差分

## C++ 对照

eloqstore 的 `tests/`、`db_stress`、`benchmark/simple_bench` 都是可执行的真实对照面。
我们的 `_cpp` 设计原本就是为 Crubit/差分准备的，但目前没有产生任何对比结果。

## 影响

当前“字节兼容”只能算代码推断：

- 没有 C++ 写 → Rust 读
- 没有 Rust 写 → C++ 读
- 没有同一 workload 的 page/manifest 字节对比
- 没有确认 branch/term、compression flag、overflow、manifest checksum 的实际布局

任何后续“对齐”改动都缺少回归判据。

## 建议

1. 先做最小 L1 fixture：
   - C++ 生成空表、单页、多页、restart、TTL、large value、archive、branch 的 page/manifest 文件
   - Rust 读取并断言 key/value/timestamp/expire_ts
2. 反向 fixture：
   - Rust 写，C++ replayer 读取并输出同样的摘要
3. 增加 `cargo xtask compat`：
   - 构建 C++ oracle
   - 生成/比对 fixtures
   - 失败时输出首个差异 offset
4. 在 CI 增加 oracle job，不能只 build。

## 验收

- CI 中存在双向交叉读写测试并通过
- `fixtures/` 有可重复生成脚本和版本号
- 至少覆盖 data/index/manifest/overflow/compression/branch 六类格式
