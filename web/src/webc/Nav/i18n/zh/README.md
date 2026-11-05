# 页面滚动时自动收起或展开的顶部导航栏

- **自动收起**：向下滚动页面时自动隐藏。
- **自动展开**：向上滚动页面或滚动条消失时自动显示。
- **性能优化**：使用 `requestAnimationFrame` 节流滚动。
- **释放资源**：组件销毁时自动移除事件监听与观察器。

## 使用演示

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">链接</a>
      </c-nav>
    </nav>
    <div class="content">往下滑动隐藏导航，往上滑动显示</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## 使用说明

必须作为 `<c-vs>` 的子元素或后代元素使用。

## 样式状态

组件在 `nav.Nav` 上切换以下类名：

- `D`：显示状态（向下滑动或在顶部）。
- `I`：正在隐藏的过渡状态。
- `H`：已隐藏状态。

## 样式变量

- `--top`：由组件自动更新为高度的负值，用于定位与过渡。