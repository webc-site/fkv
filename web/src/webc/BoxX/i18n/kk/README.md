# Жабу түймесі бар қалқымалы қабатты жасаңыз және көрсетіңіз

- Қалқымалы қабатты құру және көрсету үшін функционалды шақыру
- Кірістірілген жабу түймесі, элементті DOM ішінен жабу және жою үшін басыңыз
- Кіріс жолағын фокустау немесе қалқымалы қабатты жабу үшін Esc пернесін басыңыз

## Демонстрацияны пайдаланыңыз

```html
<button>Қалқымалы шығу үшін басыңыз</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "мәтін";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Интерфейс параметрлері

Әдепкі экспорттау функциясы `HTMLDialogElement` қайтарады және `Box` және `X` мәнер сыныптары диалогтық терезе элементіне қосылды.

## стиль сыныбы

### `.Box.X`

`dialog` элементіне қолданылады, диалогтық стильді көрсетеді.

### `a.X`

`a` элементіне қолданылады, жабу түймесі мәнерін көрсетеді.