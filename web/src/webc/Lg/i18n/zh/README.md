# 毛玻璃背景与边缘高光效果

半透明边框与内阴影构成微浮雕边缘高光。
作为按钮与输入框组件的基础样式依赖。

## 使用演示

```html
<main class="Lg">
  <article>
    <h3>标题</h3>
    <pre>内容正文</pre>
  </article>
</main>
```

## 样式类

### `.Lg`
卡片基础样式。应用 `backdrop-filter` 模糊、`border` 微浮雕边框以及 `box-shadow` 内阴影。悬浮时应用 `svg/glass.svg#h` 滤镜。

### `main.Lg`
页面主区块容器。圆角 `24px`，带响应式内边距（宽度小于 `800px` 时为 `16px`，否则为 `24px`）。

### `.Btn`, `.BtnC`, `.Input`
基础组件类名。引入本样式后，自动获得弹性布局、层级关系及毛玻璃基础属性。