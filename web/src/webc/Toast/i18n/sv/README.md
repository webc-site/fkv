# Snabbfönster i kanten av sidan

- **Funktionsanrop**: Ger normala gränssnitt och felmeddelanden
- **Auto stack**: Flera tips beräknar automatiskt avståndet och staplar vertikalt
- **Schemalagd och manuell avstängning**: Stöder schemalagd och manuell avstängning

## Använd demon

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Prompt innehåll";
});

toastErr((el) => {
  el.textContent = "Felinnehåll";
});

toast(
  (el) => {
    el.textContent = "Stängs automatiskt efter 99 sekunder";
  },
  99
);
```

## Gränssnittsparametrar

### toast(render, timeout)

Skapa och visa uppmaningar.

- `render`: `(el) => void`, rendering callback, `el` är promptbox-elementet.
- `timeout`: `Number`, timeout sekunder. Standard 9. Ställ in på 0 för att inte stängas av automatiskt.
- Returvärde: promptbox-element. Stöder `el.close()` av.

### toastErr(render, timeout)

Skapa och visa felmeddelanden. Parametrarna är samma som `toast`, med `.ERR` stilklass.

## stilklass

- `.Toast`: Promptbox basklass.
- `.ERR`: Felstatus.
- `.animated` / `.fadeInR` / `.fadeOutR`: animerad övergång.
- `.x`: Stäng-knapp.