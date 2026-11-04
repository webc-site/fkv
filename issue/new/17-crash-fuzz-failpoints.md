# 17 P1：crash harness / fuzz / failpoint 仍是骨架

## 现象

- `crate/facade/fkv_testkit/src/lib.rs` 只有一行注释
- `crate/foundation/fkv_failpoint/src/lib.rs` 只有一行
- `crate/io/fkv_io_sim/src/lib.rs` 只有一行
- workspace 没有 `fuzz/` 目录
- 没有 kill -9 / 掉电 / 磁盘满 / 慢 IO 测试

## C++ 对照

- `TEST_KILL_POINT`（AtomicWriteFile:Sync/Rename/SyncDir 等）
- `db_stress`
- 单测覆盖 crash/reopen

## 影响

- manifest/rename/GC 的崩溃一致性没有验证
- 解码器对恶意/损坏输入没有 fuzz 保护
- 24h 稳定性与磁盘满场景没有覆盖
- 现有 184 tests 都是正常路径 + 少量错误路径

## 建议

1. 实现 failpoint（编译期 feature / 运行时 env），在关键点 abort/kill：
   - data page write 前/后
   - manifest append/sync/rename/sync dir
   - branch create/delete
   - GC delete
2. crash harness：
   - 子进程执行写 workload
   - 指定 failpoint 触发 `_exit`
   - 父进程 reopen 并校验不变量
3. `cargo-fuzz`：page/manifest/recovery 解码入口。
4. 长稳：24h/7d 轮转写读 + reopen + GC。

## 验收

- CI 有 crash matrix，所有点重启后数据可读或明确回滚
- fuzz 24h 无 panic
- 24h stress 无 fd/内存泄漏
