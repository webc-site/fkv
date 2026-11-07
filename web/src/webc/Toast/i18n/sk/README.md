# Vyskakovacie okno na okraji stránky

- **Volanie funkcie**: Poskytuje normálne a chybové rozhrania
- **Automatické stohovanie**: Viaceré tipy automaticky vypočítavajú vzdialenosť a stohujú vertikálne
- **Plánované a manuálne vypnutie**: Podporuje plánované a manuálne vypnutie

## Použite demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Prompt content";
});

toastErr((el) => {
  el.textContent = "Chybný obsah";
});

toast(
  (el) => {
    el.textContent = "Automaticky zavrieť po 99 sekundách";
  },
  99
);
```

## Parametre rozhrania

### toast(render, timeout)

Vytvárajte a zobrazujte výzvy.

- `render`: `(el) => void`, vykresľovanie spätného volania, `el` je prvok poľa výzvy.
- `timeout`: `Number`, časový limit v sekundách. Predvolená hodnota 9. Nastavte na 0, aby sa automaticky nevypínal.
- Návratová hodnota: prvok poľa výzvy. Podporuje vypnutie `el.close()`.

### toastErr(render, timeout)

Vytvárajte a zobrazujte chybové hlásenia. Parametre sú rovnaké ako `toast`, s triedou štýlu `.ERR`.

## štýlová trieda

- `.Toast`: Základná trieda poľa s výzvou.
- `.ERR`: Chybový stav.
- `.animated` / `.fadeInR` / `.fadeOutR`: animovaný prechod.
- `.x`: Tlačidlo Zavrieť.