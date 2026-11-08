# 渲染组件或工具函数的文档与源码

- 双栏响应式布局，左侧显示文档，右侧显示源码
- 文档与源码支持 Edit 和 Code 快捷按钮跳转到源地址
- 顶部展示标题与简介，支持 i18n 自动翻译

## 使用演示

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## 属性参数

- `src`: 源代码的 URL 地址。
- `href`: 文档 markdown 的 URL 地址，支持用 `{code}` 占位符适配当前语言。