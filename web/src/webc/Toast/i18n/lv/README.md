# Uzvednes uznirstošais logs lapas malā

- **Funkciju izsaukums**: nodrošina parasto un kļūdu uzvednes saskarni
- **Automātiska sakraušana**: vairāki padomi automātiski aprēķina atstarpi un sakrauj vertikāli
- **Plānota un manuāla izslēgšana**: atbalsta plānoto un manuālo izslēgšanu

## Izmantojiet demonstrāciju

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Uzvedināt saturu";
});

toastErr((el) => {
  el.textContent = "Kļūdas saturs";
});

toast(
  (el) => {
    el.textContent = "Automātiski aizvērt pēc 99 sekundēm";
  },
  99
);
```

## Interfeisa parametri

### toast(render, timeout)

Izveidojiet un parādiet uzvednes.

- `render`: `(el) => void`, atzvanīšanas renderēšana, `el` ir uzvednes lodziņa elements.
- `timeout`: `Number`, noildze sekundes. Noklusējums 9. Iestatiet uz 0, lai netiktu automātiski izslēgts.
- Atgriešanas vērtība: uzvednes lodziņa elements. Atbalsta `el.close()` izslēgtu.

### toastErr(render, timeout)

Izveidojiet un parādiet kļūdu ziņojumus. Parametri ir tādi paši kā `toast` ar `.ERR` stila klasi.

## stila klase

- `.Toast`: uzvednes lodziņa pamatklase.
- `.ERR`: kļūdas statuss.
- `.animated` / `.fadeInR` / `.fadeOutR`: animēta pāreja.
- `.x`: Aizvērt poga.