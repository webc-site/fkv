# 頁面捲動時自動收起或展開的頂部導覽列

- **自動收起**：向下捲動頁面時自動隱藏。
- **自動展開**：向上捲動頁面或捲軸消失時自動顯示。
- **效能最佳化**：使用 `requestAnimationFrame` 節流捲動。
- **釋放資源**：元件銷毀時自動移除事件監聽與觀察器。

## 使用演示

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">連結</a>
      </c-nav>
    </nav>
    <div class="content">往下滑動隱藏導航，往上滑動顯示</div>
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

## 使用說明

必須作為 `<c-vs>` 的子元素或後代元素使用。

## 樣式狀態

元件在 `nav.Nav` 上切換以下類別名稱：

- `D`：顯示狀態（向下滑動或在頂部）。
- `I`：正在隱藏的過渡狀態。
- `H`：已隱藏狀態。

## 樣式變數

- `--top`：由元件自動更新為高度的負值，用於定位與過渡。