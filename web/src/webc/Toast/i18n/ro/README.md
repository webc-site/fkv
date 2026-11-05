# Fereastră pop-up prompt la marginea paginii

- **Apel de funcție**: Oferă interfețe normale și prompte de eroare
- **Stivuire automată**: Sfaturi multiple calculează automat distanța și se stivuiesc pe verticală
- **Oprire programată și manuală**: acceptă oprirea programată și manuală

## Utilizați demonstrația

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Conținut prompt";
});

toastErr((el) => {
  el.textContent = "Conținutul erorii";
});

toast(
  (el) => {
    el.textContent = "Închide automat după 99 de secunde";
  },
  99
);
```

## Parametrii interfeței

### toast(render, timeout)

Creați și afișați solicitări.

- `render`: `(el) => void`, redând apel invers, `el` este elementul casetei de prompt.
- `timeout`: `Number`, timeout secunde. Implicit 9. Setați la 0 pentru a nu se închide automat.
- Valoare returnată: element casetă prompt. Acceptă `el.close()` dezactivat.

### toastErr(render, timeout)

Creați și afișați mesaje de eroare. Parametrii sunt aceiași cu `toast`, cu clasa de stil `.ERR`.

## clasa de stil

- `.Toast`: clasă de bază pentru caseta de prompt.
- `.ERR`: starea erorii.
- `.animated` / `.fadeInR` / `.fadeOutR`: tranziție animată.
- `.x`: butonul Închidere.