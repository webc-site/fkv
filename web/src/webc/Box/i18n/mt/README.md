# Oħloq u uri saff popup

- Sejħa funzjonali biex toħloq u turi saff popup
- Neħħi awtomatikament elementi mid-DOM meta magħluq
- Prevjeni l-imġieba tal-kanċellazzjoni taċ-ċavetta Esc

## Uża d-demo

```html
<button>Ikklikkja biex titfaċċa</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Titolu";
  description.textContent = "Test";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parametri tal-interface

### Funzjonijiet default esportati

`Box()`

- **Valur tar-ritorn**: `HTMLDialogElement`, l-element tad-djalogu tal-klassi tal-istil `Box` ġie miżjud.

## klassi tal-istil

### `.Box`

Japplika għall-element `dialog` biex jissettja l-pożizzjonament, iċ-ċentru u ċ-ċajpra tal-maskra fl-isfond.

### `.Lg`

Klassi ta 'stil applikata għall-kontenut ta' ġewwa, li tistabbilixxi l-kulur ta 'l-isfond, kantunieri fit-tond, dell u tqassim vertikali, u tiddefinixxi t-tqassim ta' ġewwa `h3` u `p` .