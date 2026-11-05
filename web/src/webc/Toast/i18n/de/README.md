# Eingabeaufforderungs-Popup-Fenster am Rand der Seite

- **Funktionsaufruf**: Stellt normale und Fehleraufforderungsschnittstellen bereit
- **Automatischer Stapel**: Mehrere Spitzen berechnen automatisch den Abstand und stapeln vertikal
- **Geplantes und manuelles Herunterfahren**: Unterstützt geplantes und manuelles Herunterfahren

## Nutzen Sie die Demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Prompt-Inhalt";
});

toastErr((el) => {
  el.textContent = „Fehlerinhalt“;
});

toast(
  (el) => {
    el.textContent = „Nach 99 Sekunden automatisch schließen“;
  },
  99
);
```

## Schnittstellenparameter

### toast(render, timeout)

Eingabeaufforderungen erstellen und anzeigen.

- `render`: `(el) => void`, Rendering-Rückruf, `el` ist das Eingabeaufforderungsfeldelement.
- `timeout`: `Number`, Timeout-Sekunden. Standard 9. Auf 0 einstellen, um nicht automatisch herunterzufahren.
- Rückgabewert: Eingabeaufforderungsfeldelement. Unterstützt `el.close()` aus.

### toastErr(render, timeout)

Fehlermeldungen erstellen und anzeigen. Die Parameter sind die gleichen wie bei `toast`, mit der Stilklasse `.ERR`.

## Stilklasse

- `.Toast`: Basisklasse der Eingabeaufforderungsbox.
- `.ERR`: Fehlerstatus.
- `.animated` / `.fadeInR` / `.fadeOutR`: animierter Übergang.
- `.x`: Schaltfläche „Schließen“.