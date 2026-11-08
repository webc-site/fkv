# Sortu eta bistaratu laster-geruza bat ixteko botoi batekin

- Popup geruza sortzeko eta bistaratzeko dei funtzionala
- Itxi botoia integratua, egin klik elementua DOMetik ixteko eta kentzeko
- Sakatu Esc sarrera-koadroa desfokatzeko edo laster-geruza ixteko

## Erabili demoa

```html
<button>Egin klik agertzeko</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "testua";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Interfazearen parametroak

Esportazio funtzio lehenetsiak `HTMLDialogElement` itzultzen du, eta `Box` eta `X` estilo-klaseak gehitu dira elkarrizketa-elementuari.

## estilo klasea

### `.Box.X`

`dialog` elementuari aplikatuta, elkarrizketa-estiloa zehazten du.

### `a.X`

`a` elementuari aplikatuta, ixteko botoiaren estiloa zehazten du.