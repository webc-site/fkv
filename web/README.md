# fkv 性能看板

## 本地

```bash
cd web
bun install
bun run dev
```

`build.js` 先用 pug 渲染 `index.html`，用 `@1-/stylus` 编译 `style.css`，再交给 Vite/Svelte 打包。

## 部署

`perf.yml` 在每次 benchmark 后：

1. 下载 x86_64 / aarch64 的 `perf-run-*.json`；
2. 从 `gh-pages` 恢复历史 `perf.json`；
3. `web/scripts/merge.mjs` 合并；
4. `bun run build`；
5. 用 `GH_TOKEN` 推到 `gh-pages`。

GitHub Pages 设置：Settings → Pages → Source 选 `Deploy from a branch`，分支 `gh-pages`，目录 `/`。

## 翻译

复用 webc 流程：

```bash
WEBC_SITE_TOKEN=... bun run tran
```

配置在 `tran.yml`，文档在 `doc/<lang>/doc/`。

## 数据格式

`public/perf.json`：

```json
{
  "baseline": "<eloqstore commit>",
  "generated_at": 1793747000,
  "runs": [
    {
      "commit": "...",
      "date": 1793747000,
      "platform": "linux-x86_64",
      "impl": "rust",
      "metric": "gxhash_4096B_gib_s",
      "value": 12.4,
      "unit": "GiB/s"
    }
  ]
}
```

同一 `commit + platform + metric + impl` 只保留最新一条，曲线按 `date` 排序。
