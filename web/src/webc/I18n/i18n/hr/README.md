# Gumbi i skočni prozori za promjenu jezika sučelja

- Pritisnite gumb da biste otvorili plutajući sloj za odabir jezika.
- Automatski označi trenutno odabrani jezik.

## Koristite demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Koristite sljedeći JS kod za praćenje jezičnih promjena
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parametri sučelja

### Prilagođena oznaka `<c-i18n>`

Prilagođene HTML oznake, uvedene u JS i korištene u HTML-u.

## Upravljanje statusom

Koristite `src/js/i18n.js` za upravljanje statusom globalnog jezika:

- **`LANG_LI`**: Niz popisa jezika, u formatu `[[ime, ID], ...]`.
- **`lang()`**: Dohvatite trenutni ID jezika.
- **`langCode()`**: Preuzmite trenutno kodiranje jezika.
- **`langSet(id)`**: Postavite ID jezika i obavijestite pretplatnike.
- **`onLang(func)`**: Pretplatite se na promjene jezika. Ako je jezik postavljen, povratni poziv će se odmah pokrenuti. Vratite funkciju odjave.

## stilska klasa

### `.BtnC.lang`

ikona gumb.

### `.I18n.Lg`

Glavni izgled skočnog prozora koristi se za omotavanje jezičnih opcija.