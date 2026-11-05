# Pojavno okno s pozivom na robu strani

- **Klic funkcije**: Zagotavlja običajne vmesnike in vmesnike za obvestila o napakah
- **Samodejno zlaganje**: Več nasvetov samodejno izračuna razmik in zloži navpično
- **Načrtovana in ročna zaustavitev**: podpira načrtovano in ročno zaustavitev

## Uporabite demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Pozivna vsebina";
});

toastErr((el) => {
  el.textContent = "Vsebina napake";
});

toast(
  (el) => {
    el.textContent = "Samodejno zapri po 99 sekundah";
  },
  99
);
```

## Parametri vmesnika

### toast(render, timeout)

Ustvarite in prikažite pozive.

- `render`: `(el) => void`, upodabljanje povratnega klica, `el` je element polja za poziv.
- `timeout`: `Number`, časovna omejitev sekund. Privzeto 9. Nastavite na 0, da se ne izklopi samodejno.
- Povratna vrednost: element polja za poziv. Podpira `el.close()` off.

### toastErr(render, timeout)

Ustvarite in prikažite sporočila o napakah. Parametri so enaki kot `toast`, z razredom sloga `.ERR`.

## stilski razred

- `.Toast`: osnovni razred polja za pozive.
- `.ERR`: Stanje napake.
- `.animated` / `.fadeInR` / `.fadeOutR`: animirani prehod.
- `.x`: gumb za zapiranje.