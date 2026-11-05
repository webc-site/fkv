# Tieqa pop-up fil-pront fit-tarf tal-paġna

- **Sejħa ta' funzjoni**: Tipprovdi interfaces fil-pront normali u ta' żball
- ** Munzell awtomatiku **: Tips multipli jikkalkulaw awtomatikament l-ispazjar u munzell vertikalment
- **Tgħeluq skedat u manwali**: Jappoġġja għeluq skedat u manwali

## Uża d-demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Kontenut fil-pront";
});

toastErr((el) => {
  el.textContent = "Kontenut ta' żball";
});

toast(
  (el) => {
    el.textContent = "Agħlaq awtomatikament wara 99 sekonda";
  },
  99
);
```

## Parametri tal-interface

### toast(render, timeout)

Oħloq u juri prompts.

- `render`: `(el) => void`, li tirrendi callback, `el` hija l-element tal-kaxxa fil-pront.
- `timeout`: `Number`, sekondi ta' timeout. Default 9. Issettja għal 0 biex ma tagħlaqx awtomatikament.
- Valur tar-ritorn: element kaxxa fil-pront. Jappoġġja `el.close()` off.

### toastErr(render, timeout)

Oħloq u uri messaġġi ta 'żball. Il-parametri huma l-istess bħal `toast`, bi klassi ta' stil `.ERR`.

## klassi tal-istil

- `.Toast`: Klassi bażi tal-kaxxa fil-pront.
- `.ERR`: Stat ta' żball.
- `.animated` / `.fadeInR` / `.fadeOutR`: transizzjoni animata.
- `.x`: Buttuna Agħlaq.