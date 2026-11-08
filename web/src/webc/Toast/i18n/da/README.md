# Spørg pop op-vindue i kanten af ​​siden

- **Funktionskald**: Giver normale grænseflader og fejlmeddelelser
- **Automatisk stak**: Flere tip beregner automatisk afstanden og stakken lodret
- **Planlagt og manuel nedlukning**: Understøtter planlagt og manuel nedlukning

## Brug demoen

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Prompt indhold";
});

toastErr((el) => {
  el.textContent = "Fejlindhold";
});

toast(
  (el) => {
    el.textContent = "Lukker automatisk efter 99 sekunder";
  },
  99
);
```

## Interface parametre

### toast(render, timeout)

Opret og vis prompter.

- `render`: `(el) => void`, der gengiver tilbagekald, `el` er promptbokselementet.
- `timeout`: `Number`, timeout sekunder. Standard 9. Indstil til 0 for ikke automatisk at lukke ned.
- Returværdi: promptbokselement. Understøtter `el.close()` fra.

### toastErr(render, timeout)

Opret og vis fejlmeddelelser. Parametrene er de samme som `toast` med `.ERR` stilklasse.

## stil klasse

- `.Toast`: Promptboks basisklasse.
- `.ERR`: Fejlstatus.
- `.animated` / `.fadeInR` / `.fadeOutR`: animeret overgang.
- `.x`: Luk knap.