# Ustvarite in prikažite pojavno plast

- Funkcionalni klic za ustvarjanje in prikaz pojavne plasti
- Samodejno odstrani elemente iz DOM, ko je zaprt
- Prepreči vedenje preklica tipke Esc

## Uporabite demo

```html
<button>Kliknite za pojavno okno</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Naslov";
  description.textContent = "Besedilo";
  button.className = "Btn Main";
  button.textContent = "V redu";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parametri vmesnika

### Privzete izvožene funkcije

`Box()`

- **Povrnjena vrednost**: `HTMLDialogElement`, dodan je bil pogovorni element razreda sloga `Box`.

## stilski razred

### `.Box`

Velja za element `dialog` za nastavitev položaja, centriranja in zameglitve maske ozadja.

### `.Lg`

Razred sloga, uporabljen za notranjo vsebino, ki določa barvo ozadja, zaobljene vogale, senco in navpično postavitev ter določa postavitev notranjih `h3` in `p`.