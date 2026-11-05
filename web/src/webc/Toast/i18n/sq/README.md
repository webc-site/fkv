# Prompt pop-up dritare në skaj të faqes

- **Thirrje funksioni**: Ofron ndërfaqe normale dhe të shpejta për gabime
- **Stepi automatik**: Këshilla të shumta llogaritin automatikisht hapësirën dhe grumbullohen vertikalisht
- **Fikja e planifikuar dhe manuale **: Mbështet mbylljen e planifikuar dhe manuale

## Përdorni demonstrimin

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Përmbajtje e shpejtë";
});

toastErr((el) => {
  el.textContent = "Përmbajtja e gabimit";
});

toast(
  (el) => {
    el.textContent = "Mbyllet automatikisht pas 99 sekondash";
  },
  99
);
```

## Parametrat e ndërfaqes

### toast(render, timeout)

Krijo dhe shfaq kërkesat.

- `render`: `(el) => void`, kthimi i thirrjes, `el` është elementi i kutisë së kërkesës.
- `timeout`: `Number`, skadon sekonda. Parazgjedhja 9. Vendosni në 0 për të mos fikur automatikisht.
- Vlera e kthimit: elementi i kutisë së shpejtë. Mbështet `el.close()` joaktive.

### toastErr(render, timeout)

Krijoni dhe shfaqni mesazhe gabimi. Parametrat janë të njëjtë si `toast`, me klasën e stilit `.ERR`.

## klasë stili

- `.Toast`: Klasa bazë e kutisë së kërkesës.
- `.ERR`: Statusi i gabimit.
- `.animated` / `.fadeInR` / `.fadeOutR`: tranzicion i animuar.
- `.x`: Butoni i mbylljes.