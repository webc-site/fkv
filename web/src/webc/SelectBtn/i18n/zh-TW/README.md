# 下拉選擇框與按鈕的組合

- 將選擇框與按鈕水平拼接
- 自動儲存選取狀態至 localStorage
- 攔截按鈕點擊並派發自訂事件

## 使用演示

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">選項 1</option>
    <option value="option2">選項 2</option>
  </c-select>
  <button>運行</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## 屬性

- `key`: 用於 localStorage 持久化的鍵名
- `value`: 目前選取的值

## 事件

- `submit`: 按鈕點擊時派發，`e.value` 攜帶目前選取的值