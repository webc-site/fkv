# Vyskakovací okno na okraji stránky

- **Volání funkce**: Poskytuje rozhraní pro normální a chybová upozornění
- **Automatické stohování**: Více tipů automaticky vypočítá vzdálenost a stohuje vertikálně
- **Plánované a ruční vypnutí**: Podporuje plánované a ruční vypnutí

## Použijte demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Výzva k obsahu";
});

toastErr((el) => {
  el.textContent = "Chybný obsah";
});

toast(
  (el) => {
    el.textContent = "Automaticky zavřít po 99 sekundách";
  },
  99
);
```

## Parametry rozhraní

### toast(render, timeout)

Vytvářejte a zobrazujte výzvy.

- `render`: `(el) => void`, vykreslování zpětného volání, `el` je prvek pole výzvy.
- `timeout`: `Number`, časový limit v sekundách. Výchozí 9. Nastavením na 0 se automaticky nevypne.
- Návratová hodnota: prvek pole výzvy. Podporuje vypnutí `el.close()`.

### toastErr(render, timeout)

Vytvářejte a zobrazujte chybové zprávy. Parametry jsou stejné jako `toast`, s třídou stylu `.ERR`.

## stylová třída

- `.Toast`: Základní třída pole výzvy.
- `.ERR`: Stav chyby.
- `.animated` / `.fadeInR` / `.fadeOutR`: animovaný přechod.
- `.x`: Tlačítko Zavřít.