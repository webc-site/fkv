# Krijoni dhe shfaqni një shtresë kërcyese me një buton mbylljeje

- Thirrje funksionale për të krijuar dhe shfaqur një shtresë kërcyese
- Butoni i integruar i mbylljes, klikoni për të mbyllur dhe hequr elementin nga DOM
- Shtypni Esc për të defokusuar kutinë e hyrjes ose për të mbyllur shtresën kërcyese

## Përdorni demonstrimin

```html
<button>Klikoni për të dalë</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "tekst";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Parametrat e ndërfaqes

Funksioni i parazgjedhur i eksportit kthen `HTMLDialogElement` dhe klasat e stilit `Box` dhe `X` janë shtuar në elementin e dialogut.

## klasë stili

### `.Box.X`

Zbatuar në elementin `dialog`, specifikon stilin e dialogut.

### `a.X`

Zbatuar në elementin `a`, specifikon stilin e butonit të mbylljes.