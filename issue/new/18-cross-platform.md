# 18 P2：Windows/macOS 跨平台的三个具体阻塞点

## 现象

1. `switch_manifest` 依赖 `rename(tmp, active)` 覆盖已有文件；compio Windows 后端直接调用 `std::fs::rename`，目标存在时失败。
2. `TableIdent::new` 只拒绝空串和 `\0`，目录名直接用 `tbl_name.partition`：
   - Windows 非法字符 `: * ? " < > |`
   - macOS/Windows 大小写不敏感，`Foo`/`foo` 冲突
3. CI 只有 Ubuntu x86_64/aarch64，没有 macOS/Windows job。

## C++ 对照

eloqstore 本身只支持 Linux（io_uring + kernel 6.8+），所以这部分不是“对齐 C++”，而是 fkv 想跨平台时额外要解决的问题。

## 影响

- Windows 上第二次 manifest rotation 就会失败
- 某些表名在 macOS/Windows 无法创建目录或互相覆盖
- 没有 CI 证明 Windows 能编译/运行

## 建议

1. 实现平台原子替换：
   - Unix: `rename`
   - Windows: `ReplaceFileW` 或 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`
2. `TableIdent` 增加平台安全校验：
   - 拒绝路径分隔符、Windows 保留字符/保留名
   - 或在落盘时对 tbl_name 做 hash/encode，保持 API 名称与磁盘名解耦
3. CI 增加 macOS 测试；Windows 先 `cargo check` + 核心 CRUD 测试。
4. 文档明确 Tier 1 Linux / Tier 2 macOS / Tier 3 Windows。

## 验收

- Windows 上连续两次 manifest rotation 成功
- `Foo`/`foo` 不会静默共享目录（拒绝或区分）
- macOS CI 通过
