# Комбинација од паѓачкото поле за избор и копче

- Спојте го полето за избор и копчете хоризонтално
- Автоматски зачувајте ја избраната состојба во localStorage
- Пресретнете кликнувања на копчињата и испраќајте приспособени настани

## Користете го демо

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Опција 1</option>
    <option value="option2">Опција 2</option>
  </c-select>
  <button>трчај</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## имот

- `key`: име на клуч што се користи за упорност на локалната меморија
- `value`: моментално избрана вредност

## настан

- `submit`: се испраќа кога ќе се кликне копчето, `e.value` ја носи моментално избраната вредност