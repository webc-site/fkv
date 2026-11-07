# Promptni skočni prozor na rubu stranice

- **Poziv funkcije**: Omogućuje normalno sučelje i sučelje za obavještavanje o pogrešci
- **Automatsko slaganje**: Više savjeta automatski izračunava razmak i slaže okomito
- **Planirano i ručno isključivanje**: Podržava planirano i ručno isključivanje

## Koristite demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Promptni sadržaj";
});

toastErr((el) => {
  el.textContent = "Sadržaj pogreške";
});

toast(
  (el) => {
    el.textContent = "Automatski zatvori nakon 99 sekundi";
  },
  99
);
```

## Parametri sučelja

### toast(render, timeout)

Stvorite i prikažite upite.

- `render`: `(el) => void`, renderiranje povratnog poziva, `el` je element okvira upita.
- `timeout`: `Number`, vrijeme čekanja u sekundama. Zadano 9. Postavite na 0 da se ne isključi automatski.
- Povratna vrijednost: element okvira s upitom. Podržava `el.close()` isključeno.

### toastErr(render, timeout)

Stvaranje i prikaz poruka o pogreškama. Parametri su isti kao `toast`, s `.ERR` klasom stila.

## stilska klasa

- `.Toast`: Osnovna klasa okvira za upite.
- `.ERR`: Status pogreške.
- `.animated` / `.fadeInR` / `.fadeOutR`: animirani prijelaz.
- `.x`: Gumb za zatvaranje.