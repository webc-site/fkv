# Vytvorte a zobrazte vyskakovaciu vrstvu pomocou tlačidla Zavrieť

- Funkčné volanie na vytvorenie a zobrazenie vyskakovacej vrstvy
- Vstavané tlačidlo zatvorenia, kliknutím zatvorte a odstráňte prvok z DOM
- Stlačením Esc rozostríte vstupné pole alebo zatvoríte vyskakovaciu vrstvu

## Použite demo

```html
<button>Kliknutím sa zobrazí okno</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "text";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Parametre rozhrania

Predvolená funkcia exportu vracia `HTMLDialogElement` a do prvku dialógu boli pridané triedy štýlu `Box` a `X`.

## štýlová trieda

### `.Box.X`

Aplikované na prvok `dialog` určuje štýl dialógu.

### `a.X`

Aplikované na prvok `a` určuje štýl tlačidla zatvorenia.