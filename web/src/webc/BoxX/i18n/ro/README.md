# Creați și afișați un strat pop-up cu un buton de închidere

- Apel funcțional pentru a crea și afișa stratul pop-up
- Buton de închidere încorporat, faceți clic pentru a închide și a elimina elementul din DOM
- Apăsați Esc pentru a defocaliza caseta de introducere sau pentru a închide stratul pop-up

## Utilizați demonstrația

```html
<button>Faceți clic pentru a apărea</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = „text”;
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Parametrii interfeței

Funcția de export implicită returnează `HTMLDialogElement`, iar clasele de stil `Box` și `X` au fost adăugate elementului de dialog.

## clasa de stil

### `.Box.X`

Aplicat elementului `dialog`, specifică stilul de dialog.

### `a.X`

Aplicat elementului `a`, specifică stilul butonului de închidere.