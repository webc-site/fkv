# Krijoni dhe shfaqni një shtresë kërcyese

- Thirrje funksionale për të krijuar dhe shfaqur një shtresë kërcyese
- Hiq automatikisht elementët nga DOM kur mbyllet
- Parandaloni sjelljen e anulimit të çelësit Esc

## Përdorni demonstrimin

```html
<button>Klikoni për të dalë</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Titulli";
  description.textContent = "Tekst";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parametrat e ndërfaqes

### Funksionet e parazgjedhura të eksportuara

`Box()`

- **Vlera e kthimit**: `HTMLDialogElement`, elementi i dialogut të klasës së stilit `Box` është shtuar.

## klasë stili

### `.Box`

Zbatohet për elementin `dialog` për të vendosur pozicionimin, qendrimin dhe turbullimin e maskës së sfondit.

### `.Lg`

Një klasë stili aplikohet në përmbajtjen e brendshme, duke vendosur ngjyrën e sfondit, qoshet e rrumbullakosura, hijen dhe paraqitjen vertikale, dhe duke përcaktuar paraqitjen e `h3` dhe `p` e brendshme.