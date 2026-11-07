# 드롭다운 선택 상자와 버튼의 조합

- 선택 상자와 버튼을 가로로 이어붙이기
- 선택한 상태를 localStorage에 자동으로 저장
- 버튼 클릭을 가로채고 맞춤 이벤트 전달

## 데모 사용

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">옵션 1</option>
    <option value="option2">옵션 2</option>
  </c-select>
  <button>달리다</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## 재산

- `key`: localStorage 지속성에 사용되는 키 이름
- `value`: 현재 선택된 값

## 이벤트

- `submit`: 버튼을 클릭하면 전달됩니다. `e.value`는 현재 선택된 값을 전달합니다.