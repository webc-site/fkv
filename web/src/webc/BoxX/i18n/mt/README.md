# Oħloq u uri saff popup b'buttuna mill-qrib

- Sejħa funzjonali biex toħloq u turi saff popup
- Buttuna mill-qrib inkorporata, ikklikkja biex tagħlaq u neħħi l-element mid-DOM
- Agħfas Esc biex tiffoka l-kaxxa tad-dħul jew tagħlaq is-saff popup

## Uża d-demo

```html
<button>Ikklikkja biex titfaċċa</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "test";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Parametri tal-interface

Il-funzjoni tal-esportazzjoni default tirritorna `HTMLDialogElement`, u l-klassijiet tal-istil `Box` u `X` ġew miżjuda mal-element tad-djalogu.

## klassi tal-istil

### `.Box.X`

Applikat għall-element `dialog`, jispeċifika l-istil tad-djalogu.

### `a.X`

Applikat għall-element `a`, jispeċifika l-istil tal-buttuna tal-qrib.