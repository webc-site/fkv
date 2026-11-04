# 19 P2：fixed-buffer 依赖私有 syscall/opcode，维护成本高

## 现象

`fkv_io_compio/src/fixed.rs` 直接使用：

- `Runtime::as_raw_fd()` 取 compio runtime 的 ring fd
- `libc::syscall(SYS_io_uring_register, ...)`
- 硬编码 `IORING_REGISTER_BUFFERS2 = 15` / `UPDATE = 16`
- 自定义 `FixedRead` / `FixedWrite` 实现 `compio::driver::OpCode`
- 每个 `CompioStore` 依赖“一个 runtime 一个 ring”的假设

## compio 对照

本地 compio（0.20-beta）和 fkv 使用的 0.19.2 都没有公开：

- `register_buffers` / `unregister_buffers`
- `ReadFixed` / `WriteFixed` OpCode

只有 `Runtime::buffer_pool()` 的 provided buffer ring，且仅支持 managed read。
所以当前实现是“绕开 compio 私有边界”的合理 workaround，但：

- 依赖 opcode 编号和内部 ring fd 语义
- compio 升级可能改变 `Runtime::as_raw_fd`/driver 行为
- `FixedBuffers` drop 时用当前 runtime fd 比较，跨 runtime 使用会退化
- 不支持一个 runtime 多个 CompioStore 共享 buffer 表

## 影响

- 可维护性风险，不是当前功能 bug
- 升级 compio 时需要重新验证
- 上游若引入 fixed-buffer API，应尽快切换

## 建议

1. 把 fixed.rs 保持为独立薄层，集中所有私有假设。
2. 增加 compio 版本兼容测试（0.19/0.20）和 kernel 版本 fallback 测试。
3. 向上游提 PR：公开 `register_buffers` + `ReadFixed`/`WriteFixed`。
4. 短期在 CI 固定 compio 版本，升级时跑 Linux Docker fixed 测试。

## 验收

- compio 升级 checklist 包含 fixed IO 回归
- kernel 不支持 sparse 时自动退化到经典注册/pread/pwrite
- 多 store/多 runtime 场景有明确错误而不是未定义行为
