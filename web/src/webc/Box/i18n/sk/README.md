# Vytvorte a zobrazte vyskakovaciu vrstvu

- Funkčné volanie na vytvorenie a zobrazenie vyskakovacej vrstvy
- Po zatvorení automaticky odstrániť prvky z DOM
- Zabráňte rušeniu klávesom Esc

## Použite demo

```html
<button>Kliknutím sa zobrazí okno</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Názov";
  description.textContent = "Text";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parametre rozhrania

### Predvolené exportované funkcie

`Box()`

- **Vrátená hodnota**: `HTMLDialogElement`, bol pridaný dialógový prvok triedy štýlu `Box`.

## štýlová trieda

### `.Box`

Vzťahuje sa na prvok `dialog` na nastavenie umiestnenia, centrovania a rozostrenia masky pozadia.

### `.Lg`

Trieda štýlu aplikovaná na vnútorný obsah, ktorá nastavuje farbu pozadia, zaoblené rohy, tieň a vertikálne rozloženie a definuje rozloženie vnútorných `h3` a `p` .