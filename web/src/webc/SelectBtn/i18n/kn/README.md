# ಡ್ರಾಪ್-ಡೌನ್ ಆಯ್ಕೆ ಬಾಕ್ಸ್ ಮತ್ತು ಬಟನ್ ಸಂಯೋಜನೆ

- ಆಯ್ಕೆ ಬಾಕ್ಸ್ ಮತ್ತು ಬಟನ್ ಅನ್ನು ಅಡ್ಡಲಾಗಿ ಸ್ಪ್ಲೈಸ್ ಮಾಡಿ
- ಸ್ಥಳೀಯ ಸಂಗ್ರಹಣೆಗೆ ಆಯ್ಕೆಮಾಡಿದ ಸ್ಥಿತಿಯನ್ನು ಸ್ವಯಂಚಾಲಿತವಾಗಿ ಉಳಿಸಿ
- ಬಟನ್ ಕ್ಲಿಕ್‌ಗಳನ್ನು ಪ್ರತಿಬಂಧಿಸಿ ಮತ್ತು ಕಸ್ಟಮ್ ಈವೆಂಟ್‌ಗಳನ್ನು ರವಾನಿಸಿ

## ಡೆಮೊ ಬಳಸಿ

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">ಆಯ್ಕೆ 1</option>
    <option value="option2">ಆಯ್ಕೆ 2</option>
  </c-select>
  <button>ಓಡುತ್ತಾರೆ</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ಆಸ್ತಿ

- `key`: ಲೋಕಲ್ ಸ್ಟೋರೇಜ್ ನಿರಂತರತೆಗಾಗಿ ಬಳಸಲಾದ ಪ್ರಮುಖ ಹೆಸರು
- `value`: ಪ್ರಸ್ತುತ ಆಯ್ಕೆಮಾಡಿದ ಮೌಲ್ಯ

## ಘಟನೆ

- `submit`: ಬಟನ್ ಅನ್ನು ಕ್ಲಿಕ್ ಮಾಡಿದಾಗ ಕಳುಹಿಸಲಾಗುತ್ತದೆ, `e.value` ಪ್ರಸ್ತುತ ಆಯ್ಕೆಮಾಡಿದ ಮೌಲ್ಯವನ್ನು ಹೊಂದಿರುತ್ತದೆ