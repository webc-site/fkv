# Kumbinasyon ng drop-down selection box at button

- Pagdugtong nang pahalang ang kahon ng pagpili at pindutan
- Awtomatikong i-save ang napiling estado sa localStorage
- Harangin ang mga pag-click sa button at ipadala ang mga custom na kaganapan

## Gamitin ang demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opsyon 1</option>
    <option value="option2">Opsyon 2</option>
  </c-select>
  <button>tumakbo</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ari-arian

- `key`: key name na ginamit para sa localStorage persistence
- `value`: kasalukuyang napiling halaga

## kaganapan

- `submit`: Ipinadala kapag na-click ang button, ang `e.value` ay nagdadala ng kasalukuyang napiling halaga