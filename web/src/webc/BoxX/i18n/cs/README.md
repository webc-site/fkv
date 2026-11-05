# Vytvořte a zobrazte vyskakovací vrstvu pomocí tlačítka Zavřít

- Funkční volání pro vytvoření a zobrazení vyskakovací vrstvy
- Vestavěné tlačítko Zavřít, kliknutím zavřete a odeberete prvek z DOM
- Stisknutím Esc rozestřete vstupní pole nebo zavřete vyskakovací vrstvu

## Použijte demo

```html
<button>Kliknutím zobrazíte vyskakovací okno</button>

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


## Parametry rozhraní

Výchozí funkce exportu vrací `HTMLDialogElement` a do prvku dialogu byly přidány třídy stylu `Box` a `X`.

## stylová třída

### `.Box.X`

Aplikováno na prvek `dialog`, určuje styl dialogu.

### `a.X`

Aplikováno na prvek `a`, určuje styl tlačítka pro zavření.