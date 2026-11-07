# Креирајте и прикажете скокачки слој со копче за затворање

- Функционален повик за креирање и прикажување на скокачки слој
- Вградено копче за затворање, кликнете за да го затворите и отстраните елементот од DOM
- Притиснете Esc за да го дефокусирате полето за внесување или да го затворите скокачкиот слој

## Користете го демо

```html
<button>Кликнете за да се појави</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "текст";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Параметри на интерфејсот

Стандардната функција за извоз враќа `HTMLDialogElement`, а класите на стилови `Box` и `X` се додадени во елементот за дијалог.

## класа стил

### `.Box.X`

Применет на елементот `dialog`, го одредува стилот на дијалогот.

### `a.X`

Применет на елементот `a`, го одредува стилот на копчето за затворање.