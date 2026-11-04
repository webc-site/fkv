# fkv 性能看板

Rust 版 eloqstore 的每次提交性能曲线，以及与 C++ oracle 的相对比值。

- `web/public/perf.json`：性能历史，由 `perf.yml` 更新。
- `src/App.svelte`：看板页面。
- `src/styl/`：复用 webc 的 reset / form / button / card / kv 样式。
- `sh/tran.js`：复用 webc 翻译流程，`tran.yml` 配置目录。
