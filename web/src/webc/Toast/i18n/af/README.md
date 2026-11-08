# Vinnige opspringvenster aan die rand van die bladsy

- **Funksie-oproep**: Verskaf normale en foutboodskap-koppelvlakke
- **Outostapel**: Veelvuldige wenke bereken outomaties die spasiëring en stapel vertikaal
- **Geskeduleerde en handmatige afskakeling**: Ondersteun geskeduleerde en handmatige afsluiting

## Gebruik die demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Prompt inhoud";
});

toastErr((el) => {
  el.textContent = "Foutinhoud";
});

toast(
  (el) => {
    el.textContent = "Sluit outomaties na 99 sekondes";
  },
  99
);
```

## Interface parameters

### toast(render, timeout)

Skep en vertoon opdragte.

- `render`: `(el) => void`, wat terugbel lewer, `el` is die promptboks-element.
- `timeout`: `Number`, uittel sekondes. Verstek 9. Stel op 0 om nie outomaties af te sluit nie.
- Terugkeerwaarde: promptbokselement. Ondersteun `el.close()` af.

### toastErr(render, timeout)

Skep en vertoon foutboodskappe. Die parameters is dieselfde as `toast`, met `.ERR` stylklas.

## styl klas

- `.Toast`: Prompt box basisklas.
- `.ERR`: Foutstatus.
- `.animated` / `.fadeInR` / `.fadeOutR`: geanimeerde oorgang.
- `.x`: Maak toe-knoppie.