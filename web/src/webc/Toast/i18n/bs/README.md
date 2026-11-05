# Prompt pop-up prozor na ivici stranice

- **Poziv funkcije**: Pruža normalne i brze interfejse za greške
- **Automatsko slaganje**: Višestruki savjeti automatski izračunavaju razmak i naslagaju okomito
- **Planirano i ručno isključivanje**: Podržava zakazano i ručno isključivanje

## Koristite demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Sadržaj upita";
});

toastErr((el) => {
  el.textContent = "Sadržaj greške";
});

toast(
  (el) => {
    el.textContent = "Automatski zatvori nakon 99 sekundi";
  },
  99
);
```

## Parametri interfejsa

### toast(render, timeout)

Kreirajte i prikažite upite.

- `render`: `(el) => void`, renderiranje povratnog poziva, `el` je element okvira za prompt.
- `timeout`: `Number`, vremensko ograničenje sekunde. Podrazumevano 9. Postavite na 0 da se ne isključi automatski.
- Povratna vrijednost: element prompt box-a. Podržava `el.close()` isključeno.

### toastErr(render, timeout)

Kreirajte i prikažite poruke o greškama. Parametri su isti kao `toast`, sa `.ERR` klasom stila.

## stilska klasa

- `.Toast`: Osnovna klasa okvira upita.
- `.ERR`: Status greške.
- `.animated` / `.fadeInR` / `.fadeOutR`: animirani prijelaz.
- `.x`: Dugme za zatvaranje.