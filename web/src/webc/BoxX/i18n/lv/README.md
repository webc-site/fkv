# Izveidojiet un parādiet uznirstošo slāni ar aizvēršanas pogu

- Funkcionāls zvans, lai izveidotu un parādītu uznirstošo slāni
- Iebūvēta aizvēršanas poga, noklikšķiniet, lai aizvērtu un noņemtu elementu no DOM
- Nospiediet Esc, lai defokusētu ievades lodziņu vai aizvērtu uznirstošo slāni

## Izmantojiet demonstrāciju

```html
<button>Noklikšķiniet, lai uznirst</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "teksts";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Interfeisa parametri

Noklusējuma eksportēšanas funkcija atgriež `HTMLDialogElement`, un stila klases `Box` un `X` ir pievienotas dialoga elementam.

## stila klase

### `.Box.X`

Lieto elementam `dialog`, norāda dialoga stilu.

### `a.X`

Lieto elementam `a`, norāda aizvēršanas pogas stilu.