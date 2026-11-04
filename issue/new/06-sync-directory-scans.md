# 06 P1：异步路径里使用同步 read_dir / metadata

## 现象

`CompioStore` 的目录索引和目录存在性检查使用同步 `std::fs`：

```rust
// crate/io/fkv_io_compio/src/lib.rs:125
if let Ok(entry_li) = read_dir(self.part_dir(tbl)) { ... }

fn dir_stamp(...) {
  let meta = std::fs::metadata(self.part_dir(tbl)).ok()?;
}
```

`ensure_dir` 也是同步 `create_dir_all`。

这些函数在 compio shard 线程的 async 调用栈中执行，直接阻塞 reactor。

## C++ 对照

C++ `IouringMgr` 使用目录 FD + io_uring 的 `getdents` / `openat` / `statx`，目录扫描和 FD 打开都是异步的；同时维护 table 目录 FD cache。

## 影响

- 大目录 (上万文件) 首次 path cache miss 时，整个 shard 停住
- `preload_paths` 在请求路径上也可能触发同步扫描
- 与 compio “不要阻塞 reactor” 的模型冲突
- 与 02/03 叠加后，单次 get 的阻塞被放大

## 建议

1. attach/reopen 阶段预加载目录索引，热路径不再 scan。
2. 必须在运行期扫描时用 `compio::runtime::spawn_blocking`，把同步 `read_dir` 移出 reactor。
3. 目录存在性/创建缓存化，避免每次 write 都 `create_dir_all`。
4. 长期维护 table 目录 FD（见 04），用异步 getdents/statx。

## 验收

- 大目录首次读请求不阻塞其他 shard 请求
- tracing 中 async 线程没有 `read_dir/stat` 同步 syscall
- 目录变更检测测试仍通过
