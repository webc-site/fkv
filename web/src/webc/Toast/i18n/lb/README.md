# Prompt Pop-up Fënster um Rand vun der Säit

- ** Funktioun Uruff **: Bitt normal a Feeler Prompt Interfaces
- ** Auto Stack **: Multiple Tipps berechent automatesch d'Distanz a stackelt vertikal
- ** Geplangte a manuelle Shutdown **: Ënnerstëtzt geplangten a manuelle Shutdown

## Benotzt d'Demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Prompt Inhalt";
});

toastErr((el) => {
  el.textContent = "Feeler Inhalt";
});

toast(
  (el) => {
    el.textContent = "Automatesch no 99 Sekonnen zou";
  },
  99
);
```

## Interface Parameteren

### toast(render, timeout)

Erstellt a weist Ufroen.

- `render`: `(el) => void`, rendering callback, `el` ass d'Promptbox Element.
- `timeout`: `Number`, Timeout Sekonnen. Standard 9. Setzt op 0 fir net automatesch auszeschalten.
- Retour Wäert: VIRSCHLOEN Këscht Element. Ënnerstëtzt `el.close()` aus.

### toastErr(render, timeout)

Erstellt a weist Fehlermeldungen. D'Parameteren sinn d'selwecht wéi `toast`, mat `.ERR` Stilklass.

## Stil Klass

- `.Toast`: Prompt Box base class.
- `.ERR`: Feelerstatus.
- `.animated` / `.fadeInR` / `.fadeOutR`: animéierten Iwwergang.
- `.x`: Knäppchen zoumaachen.