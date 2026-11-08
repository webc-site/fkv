# Viip hüpikaken lehe servas

- **Funktsioonikutse**: pakub tavalisi ja veateate liideseid
- **Automaatne virnastamine**: mitu otsa arvutab automaatselt vahekauguse ja virnastab vertikaalselt
- **Ajastatud ja käsitsi väljalülitamine**: toetab ajastatud ja käsitsi väljalülitamist

## Kasutage demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Sisu küsimine";
});

toastErr((el) => {
  el.textContent = "Viga sisu";
});

toast(
  (el) => {
    el.textContent = "Sule automaatselt 99 sekundi pärast";
  },
  99
);
```

## Liidese parameetrid

### toast(render, timeout)

Looge ja kuvage viipasid.

- `render`: `(el) => void`, renderdab tagasihelistamist, `el` on viibakasti element.
- `timeout`: `Number`, aegumissekundid. Vaikimisi 9. Seadistage 0, et mitte automaatselt välja lülituda.
- Tagastatav väärtus: viipakasti element. Toetab `el.close()` välja.

### toastErr(render, timeout)

Looge ja kuvage veateateid. Parameetrid on samad, mis `toast`, stiiliklassiga `.ERR`.

## stiiliklass

- `.Toast`: viipakasti põhiklass.
- `.ERR`: Vea olek.
- `.animated` / `.fadeInR` / `.fadeOutR`: animeeritud üleminek.
- `.x`: sulgemisnupp.