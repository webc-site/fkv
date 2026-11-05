# Combination of drop-down selection box and button

- Splice the selection box and button horizontally
- Automatically save the selected state to localStorage
- Intercept button clicks and dispatch custom events

## Use the demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Option 1</option>
    <option value="option2">Option 2</option>
  </c-select>
  <button>Run</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## Property

- `key`: key name used for localStorage persistence
- `value`: currently selected value

## Event

- `submit`: Dispatched when the button is clicked, `e.value` carries the currently selected value