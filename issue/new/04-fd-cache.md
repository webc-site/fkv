# 04 P1：每次 page IO 都重新 open，没有 FD cache

## 现象

`CompioStore` 的读写路径每次都 `File::open` / `OpenOptions::open`：

- `read_page`：`open_data_file` 打开文件
- `write_page`：`OpenOptions::new().open(...)`
- `read_segment` / `write_segment`、manifest 读写同样每次 open
- `fkv_io_compio/src/lib.rs` 顶部注释也写着“后续补 FD LRU”

## C++ 对照

eloqstore 的 `IouringMgr` 维护 FD cache：

- `OpenFD` 按 `(table, file type, branch, term)` 复用
- 引用计数 + LRU
- 支持 registered/fixed file，避免重复 openat

参见 `docs/architecture/07-io-stack.md` 的 FD cache 部分。

## 影响

- 每次 4 KiB page IO 都伴随 open/close syscall 和路径解析
- 目录项缓存压力大，page cache 命中被 open 开销抵消
- 无法利用 compio/io_uring 的 fixed-file 能力
- 高并发 scan/get 时 syscall 占比明显

## 建议

1. 在 `CompioStore` 增加 `(TableIdent, FileId, branch, term) -> SharedFd/File` 缓存。
2. 使用 LRU + 引用计数，remove/branch/term 变化时失效。
3. 读写路径从缓存取 fd，避免每次 open。
4. 评估 compio `Runtime::register_files` + 自定义 fixed-file op；当前内置 `ReadAt` 不用 file_index，不能直接依赖。

## 验收

- 连续 10k 次 get 的 open 次数为 O(file 数)，不是 O(IO 数)
- GC/remove 后 fd 缓存无泄漏
- fd 上限测试通过
