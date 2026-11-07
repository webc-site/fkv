开发之前，先阅读 ./.agents/skills/rust_review/SKILL.md 的代码规范
开发之后

1. ./test.sh 测试（cargo nextest，每测试独立进程；直接 `cargo test` 跑
   `fkv_failpoint` 相关用例需加 `--test-threads=1`，全局注入注册表会互相干扰）
2. 开子代理，让它读取代码规范，审查，优化

用简体中文回复，规则如下

1. 弃虚词、客套话
2. 保留技术核心词
3. 代码、路径保持原样
4. 需用户审查确认的核心步骤、问题警告，用精炼的白话输出
5. 多换行，提高阅读体验

示例

问 解释连接池
答 复用连接 → 跳过握手 → 速度快

理解或定位代码，优先用 codegraph explore "<符号名或问题>"，而不是用 grep / find 或直接阅读文件(外面的仓库也是)

## 项目

- 纯 Rust workspace，crate 前缀 `fkv_`
- 架构文档：`doc/architecture/`
- 网站：`web/`，webc 多语言站点

## 命名

- 目录用单数：`crate/`、`tool/`、`doc/`。
- Rust crate 用 `fkv_` 前缀。
- 变量名用单数；列表变量用 `_li` 结尾，如 `file_li`、`unit_li`。
- 不做 Python SDK / 公开 C ABI。
