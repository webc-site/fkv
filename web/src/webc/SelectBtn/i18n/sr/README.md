# Комбинација падајућег оквира за избор и дугмета

- Спојите оквир за избор и дугме хоризонтално
- Аутоматски сачувајте изабрано стање у лоцалСтораге
- Пресретните кликове на дугме и отпремите прилагођене догађаје

## Користите демо

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Опција 1</option>
    <option value="option2">Опција 2</option>
  </c-select>
  <button>трчи</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## имовине

- <б ид="1">: име кључа који се користи за постојаност лоцалСтораге
- <б ид="1">: тренутно изабрана вредност

## догађај

- <б ид="1">: Шаље се када се кликне на дугме, <б ид="2"> носи тренутно изабрану вредност