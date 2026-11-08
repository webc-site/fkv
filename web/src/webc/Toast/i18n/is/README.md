# Hvetjandi sprettigluggi á jaðri síðunnar

- ** Virka kalla**: Býður upp á eðlileg viðmót og villuboð
- **Sjálfvirk stafla**: Margar ábendingar reikna sjálfkrafa út bilið og stafla lóðrétt
- **Tímasett og handvirk lokun**: Styður áætlaða og handvirka lokun

## Notaðu kynninguna

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Hvetjandi efni";
});

toastErr((el) => {
  el.textContent = "Villa innihald";
});

toast(
  (el) => {
    el.textContent = "Loka sjálfkrafa eftir 99 sekúndur";
  },
  99
);
```

## Viðmótsbreytur

### toast(render, timeout)

Búðu til og birtu leiðbeiningar.

- `render`: `(el) => void`, sem gefur svarhringingu, `el` er kvaðningarreiturinn.
- `timeout`: `Number`, tímamörk sekúndur. Sjálfgefið 9. Stillt á 0 til að slökkva ekki sjálfkrafa.
- Skilagildi: frumefni hvetja kassi. Styður `el.close()` slökkt.

### toastErr(render, timeout)

Búðu til og birtu villuboð. Færibreyturnar eru þær sömu og `toast`, með `.ERR` stílflokki.

## stílaflokkur

- `.Toast`: Grunnflokkur hvetja kassi.
- `.ERR`: Villustaða.
- `.animated` / `.fadeInR` / `.fadeOutR`: hreyfimyndaskipti.
- `.x`: Lokahnappur.