# Dugmad i iskačući prozori za promjenu jezika interfejsa

- Kliknite na dugme da biste otvorili plutajući sloj za izbor jezika.
- Automatski označite trenutno odabrani jezik.

## Koristite demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Koristite sljedeći JS kod za praćenje promjena jezika
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parametri interfejsa

### Prilagođena oznaka `<c-i18n>`

Prilagođene HTML oznake, uvedene u JS i korištene u HTML-u.

## Upravljanje statusom

Koristite `src/js/i18n.js` za upravljanje statusom globalnog jezika:

- **`LANG_LI`**: Niz liste jezika, u formatu `[[ime, ID], ...]`.
- **`lang()`**: Dobijte ID trenutnog jezika.
- **`langCode()`**: Uzmite trenutno kodiranje jezika.
- **`langSet(id)`**: Postavite ID jezika i obavijestite pretplatnike.
- **`onLang(func)`**: Pretplatite se na promjene jezika. Ako je jezik podešen, povratni poziv će se odmah pokrenuti. Vratite funkciju odjave.

## stilska klasa

### `.BtnC.lang`

ikona dugmeta.

### `.I18n.Lg`

Glavni izgled iskačućeg prozora se koristi za omotavanje jezičkih opcija.