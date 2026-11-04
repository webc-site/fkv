# C++ shim

每个 `_cpp` crate 的 C++ 入口。目标是把 eloqstore 的复杂 C++ 类型收敛成
Crubit 可生成的简单签名：

- `std::string_view` / `std::span<const uint8_t>` 输入；
- POD 或 opaque `void*` 输出；
- 不跨边界暴露 `std::function`、`boost::context`、absl 容器。

真正的 shim 随对应 crate 的迁移任务逐个补齐。
