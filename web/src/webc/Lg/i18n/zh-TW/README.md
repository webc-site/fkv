# 毛玻璃背景與邊緣高光效果

半透明邊框與內陰影構成微浮雕邊緣高光。
作為按鈕與輸入框元件的基礎樣式相依。

## 使用演示

```html
<main class="Lg">
  <article>
    <h3>標題</h3>
    <pre>內容正文</pre>
  </article>
</main>
```

## 樣式類

### `.Lg`
卡片基礎樣式。塗上 `backdrop-filter` 模糊、`border` 微浮雕邊框以及 `box-shadow` 內陰影。懸浮時應用 `svg/glass.svg#h` 濾鏡。

### `main.Lg`
頁面主區塊容器。圓角 `24px`，帶響應式內邊距（寬度小於 `800px` 時為 `16px`，否則為 `24px`）。

### `.Btn`, `.BtnC`, `.Input`
基礎元件類別名稱。引入本樣式後，自動取得彈性佈局、層級關係及毛玻璃基礎屬性。