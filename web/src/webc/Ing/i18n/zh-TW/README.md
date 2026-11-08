# 處於載入或處理狀態的指示器

- 統一遊標為等待狀態
- 顯示波紋動畫指示處理狀態
- 為表單套用模糊遮罩，防止重複提交

## 使用演示

```html
<!-- 獨立加載指示器 -->
<div class="Ing"></div>

<!-- 帶遮罩的加載中表單 -->
<form class="Ing">
  <input type="text" placeholder="輸入框">
  <button type="submit">提交</button>
</form>
```

## 樣式類

### `.Ing`
- 指標狀態為 `wait`。
- 偽元素 `::before` 渲染波紋動畫。
- 用於 `form` 元素時，偽元素 `::after` 渲染 1px 模糊遮罩，停用互動與文字選擇。