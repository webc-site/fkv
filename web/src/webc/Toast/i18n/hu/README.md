# Felugró ablak az oldal szélén

- **Funkcióhívás**: Normál és hibajelző interfészt biztosít
- **Automatikus halmozás**: Több hegy automatikusan kiszámítja a távolságot, és függőlegesen rakja egymásra
- **Ütemezett és kézi leállítás**: Támogatja az ütemezett és kézi leállítást

## Használd a demót

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Tartalom kérése";
});

toastErr((el) => {
  el.textContent = "Tartalomhiba";
});

toast(
  (el) => {
    el.textContent = "Automatikus bezárás 99 másodperc után";
  },
  99
);
```

## Interfész paraméterei

### toast(render, timeout)

Üzenetek létrehozása és megjelenítése.

- `render`: `(el) => void`, visszahívás megjelenítése, `el` a prompt doboz elem.
- `timeout`: `Number`, időtúllépés másodpercek. Alapértelmezés 9. Állítsa 0-ra, hogy ne kapcsoljon ki automatikusan.
- Visszatérési érték: prompt box elem. Támogatja az `el.close()` kikapcsolást.

### toastErr(render, timeout)

Hibaüzenetek létrehozása és megjelenítése. A paraméterek megegyeznek az `toast` paraméterrel, `.ERR` stílusosztályszal.

## stílus osztály

- `.Toast`: Prompt box alaposztály.
- `.ERR`: Hibaállapot.
- `.animated` / `.fadeInR` / `.fadeOutR`: animált átmenet.
- `.x`: Bezárás gomb.