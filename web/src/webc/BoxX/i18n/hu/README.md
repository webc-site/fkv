# Hozzon létre és jelenítsen meg egy felugró réteget a bezárás gombbal

- Funkcionális hívás előugró réteg létrehozásához és megjelenítéséhez
- Beépített bezárás gomb, kattintson az elem bezárásához és eltávolításához a DOM-ból
- Nyomja meg az Esc billentyűt a beviteli mező defókuszálásához vagy a felugró réteg bezárásához

## Használd a demót

```html
<button>Kattintson a felbukkanáshoz</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "szöveg";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Interfész paraméterei

Az alapértelmezett exportálási függvény a `HTMLDialogElement` értéket adja vissza, és a `Box` és `X` stílusosztályok hozzáadásra kerültek a párbeszédpanel elemhez.

## stílus osztály

### `.Box.X`

Az `dialog` elemre alkalmazva megadja a párbeszédpanel stílusát.

### `a.X`

Az `a` elemre alkalmazva megadja a bezárás gomb stílusát.