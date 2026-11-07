# ドロップダウン選択ボックスとボタンの組み合わせ

- 選択ボックスとボタンを水平方向に結合します
- 選択した状態を localStorage に自動的に保存します
- ボタンのクリックをインターセプトし、カスタム イベントをディスパッチします

## デモを使用する

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">オプション 1</option>
    <option value="option2">オプション 2</option>
  </c-select>
  <button>走る</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## 財産

- `key`: localStorage の永続化に使用されるキー名
- `value`: 現在選択されている値

## イベント

- `submit`: ボタンをクリックすると送出され、`e.value` には現在選択されている値が格納されます。