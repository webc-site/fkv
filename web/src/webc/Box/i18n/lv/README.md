# Izveidojiet un parādiet uznirstošo slāni

- Funkcionāls zvans, lai izveidotu un parādītu uznirstošo slāni
- Kad tas ir aizvērts, automātiski noņemiet elementus no DOM
- Novērst Esc taustiņu atcelšanas darbību

## Izmantojiet demonstrāciju

```html
<button>Noklikšķiniet, lai uznirst</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Nosaukums";
  description.textContent = "Teksts";
  button.className = "Btn Main";
  button.textContent = "Labi";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Interfeisa parametri

### Eksportētās noklusējuma funkcijas

`Box()`

- **Atgriešanas vērtība**: `HTMLDialogElement`, ir pievienots `Box` stila klases dialoga elements.

## stila klase

### `.Box`

Attiecas uz elementu `dialog`, lai iestatītu pozicionēšanu, centrēšanu un fona maskas aizmiglojumu.

### `.Lg`

Stila klase, kas tiek lietota iekšējam saturam, iestatot fona krāsu, noapaļotus stūrus, ēnu un vertikālo izkārtojumu un definējot iekšējo `h3` un `p` izkārtojumu.