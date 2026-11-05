# Vytvořte a zobrazte vyskakovací vrstvu

- Funkční volání pro vytvoření a zobrazení vyskakovací vrstvy
- Automaticky odstranit prvky z DOM při zavření
- Zabránit chování při zrušení klávesy Esc

## Použijte demo

```html
<button>Kliknutím zobrazíte vyskakovací okno</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Titul";
  description.textContent = "Text";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parametry rozhraní

### Výchozí exportované funkce

`Box()`

- **Návratová hodnota**: `HTMLDialogElement`, byl přidán dialogový prvek třídy stylu `Box`.

## stylová třída

### `.Box`

Platí pro prvek `dialog` pro nastavení umístění, centrování a rozostření masky pozadí.

### `.Lg`

Třída stylu aplikovaná na vnitřní obsah, nastavující barvu pozadí, zaoblené rohy, stín a vertikální rozvržení a definující rozvržení vnitřních `h3` a `p` .