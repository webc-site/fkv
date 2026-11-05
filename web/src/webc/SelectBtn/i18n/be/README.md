# Камбінацыя выпадальнага поля выбару і кнопкі

- Злучыце поле выбару і кнопку гарызантальна
- Аўтаматычна захоўваць выбраны стан у localStorage
- Перахопліваць націскі кнопак і адпраўляць карыстальніцкія падзеі

## Выкарыстоўвайце дэма

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Варыянт 1</option>
    <option value="option2">Варыянт 2</option>
  </c-select>
  <button>бегчы</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## уласнасць

- `key`: імя ключа, якое выкарыстоўваецца для захоўвання localStorage
- `value`: выбранае значэнне

## мерапрыемства

- `submit`: адпраўляецца пры націсканні кнопкі, `e.value` нясе выбранае ў цяперашні час значэнне