# Iššokantis langas puslapio krašte

- **Funkcijų iškvietimas**: suteikia įprastas ir klaidų iškvietimo sąsajas
- **Automatinis sudėjimas**: keli antgaliai automatiškai apskaičiuoja tarpus ir sukrauna vertikaliai
- **Suplanuotas ir rankinis išjungimas**: palaiko suplanuotą ir rankinį išjungimą

## Naudokite demonstracinę versiją

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Ragintas turinys";
});

toastErr((el) => {
  el.textContent = "Klaidos turinys";
});

toast(
  (el) => {
    el.textContent = "Automatiškai uždaryti po 99 sekundžių";
  },
  99
);
```

## Sąsajos parametrai

### toast(render, timeout)

Kurti ir rodyti raginimus.

- `render`: `(el) => void`, atvaizduojamas atgalinis skambutis, `el` yra raginimo laukelio elementas.
- `timeout`: `Number`, skirtasis laikas sekundės. Numatytasis 9. Nustatykite 0, kad nebūtų automatiškai išjungtas.
- Grąžinama vertė: raginimo dėžutės elementas. Palaikoma `el.close()` išjungta.

### toastErr(render, timeout)

Kurti ir rodyti klaidų pranešimus. Parametrai yra tokie patys kaip `toast`, su `.ERR` stiliaus klase.

## stiliaus klasė

- `.Toast`: raginimo laukelio pagrindinė klasė.
- `.ERR`: klaidos būsena.
- `.animated` / `.fadeInR` / `.fadeOutR`: animuotas perėjimas.
- `.x`: mygtukas Uždaryti.