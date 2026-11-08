# 可切換的標籤頁

- 標籤選取狀態與下方內容區域無縫融為一體。
- 結構規範：子元素固定採用 `nav > a` 作為標籤導航，直接子元素 `b` 作為內容區域。
- 自動關聯 `a[value]` 標籤與 `b[slot]` 內容節點。
- 支援透過 `key` 屬性自動持久化目前啟動頁到 `localStorage` 中。
- 支援透過 `value` 屬性指定啟動的標籤頁（若無快取及 `value`，則預設選取帶有 `class="A"` 的標籤，或首個標籤）。
- 輕量化 Light DOM 實現，無 Shadow DOM 阻隔，樣式客製化自由度高。

## 使用演示

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">蘋果</a>
    <a value="banana">香蕉</a>
    <a value="orange">橘子</a>
  </nav>
  <b slot="apple">蘋果是一種營養豐富的水果</b>
  <b slot="banana">香蕉是熱帶地區的水果</b>
  <b slot="orange">橘子含有豐富的維生素 C</b>
</c-tab>
```

## 屬性

- `key`: 用於 localStorage 持久化的鍵名
- `value`: 目前選取的值

## 自訂事件

- `change`: 標籤頁切換時觸發，`e.value` 為目前啟動標籤的 `value` 值。