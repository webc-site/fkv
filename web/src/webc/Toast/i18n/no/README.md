# Sprett popup-vindu på kanten av siden

- **Funksjonsanrop**: Gir normale grensesnitt og grensesnitt for feilmeldinger
- **Automatisk stabel**: Flere tips beregner automatisk avstanden og stablen vertikalt
- **Planlagt og manuell avstengning**: Støtter planlagt og manuell avslutning

## Bruk demoen

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Prompt innhold";
});

toastErr((el) => {
  el.textContent = "Feilinnhold";
});

toast(
  (el) => {
    el.textContent = "Lukk automatisk etter 99 sekunder";
  },
  99
);
```

## Grensesnittparametere

### toast(render, timeout)

Opprett og vis ledetekster.

- `render`: `(el) => void`, som gjengir tilbakeringing, `el` er ledetekstbokselementet.
- `timeout`: `Number`, tidsavbrudd sekunder. Standard 9. Sett til 0 for ikke å slå seg av automatisk.
- Returverdi: ledetekstbokselement. Støtter `el.close()` av.

### toastErr(render, timeout)

Opprett og vis feilmeldinger. Parametrene er de samme som `toast`, med `.ERR` stilklasse.

## stilklasse

- `.Toast`: Grunnklasse for ledetekst.
- `.ERR`: Feilstatus.
- `.animated` / `.fadeInR` / `.fadeOutR`: animert overgang.
- `.x`: Lukk-knapp.