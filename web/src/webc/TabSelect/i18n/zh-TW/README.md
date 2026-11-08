# 帶有下拉選擇的標籤頁

- 支援在標籤內嵌入自適應寬度下拉選擇框（`<c-select>`），僅在目前標籤頁啟動時展示。
- 結構規格：子元素採用 `nav > a` 作為標籤導航，直接子元素 `b` 作為內容區域。
- 自動關聯 `a[value]` 標籤、帶有 `c-select` 的 `a`（依據組的 value 屬性與選取 option 值）與 `b[slot]` 內容節點。
- 支援透過 `key` 屬性自動持久化目前活化項到 `localStorage` 中。
- 支援使用 `key + '/' + tab` 自動儲存和恢復每一組的歷史子選擇項到 `localStorage`。
- 支援透過 `value` 屬性指定啟動的標籤頁。

## 使用演示

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>語言</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>系統</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript 是一種腳本語言</b>
    <b slot="rs">Rust 是一種系統級程式語言</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS 是基於 Unix 的圖形作業系統</b>
    <b slot="win">Windows 是微軟開發的作業系統</b>
  </b>
</c-tab-select>
```

## 屬性

- `key`: 用於 localStorage 持久化的鍵名
- `value`: 目前選取的值

## 自訂事件

- `change`: 標籤頁切換時觸發，`e.value` 為目前啟動選項的 `value` 值。