# fkv 网站

webc 站点，源码结构与 `/Users/z/git/webc` 一致。

- `doc/<lang>/doc/*.md`：多语言文档，`doc/doc.yml` 定义导航。
- `doc/zh` 是源语言；翻译由 `webc tran` 生成，词条缓存写入 YAML，不使用 JSON。
- `src/`：webc 组件与页面壳。
- `sh/`：webc CLI 入口。

## 本地

```bash
cd web
bun install
bun run dev
```

## 构建

```bash
cd web
bun run build
# 项目页部署到 /fkv/ 时：
CDN=/fkv/ bun run build
```

产物在 `web/dist/dist`。

## 翻译

```bash
cd web
WEBC_SITE_TOKEN=... bun run tran
```
