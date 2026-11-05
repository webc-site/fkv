# Butoane și ferestre pop-up pentru comutarea limbilor interfeței

- Faceți clic pe butonul pentru a afișa stratul plutitor de selecție a limbii.
- Evidențiați automat limba selectată în prezent.

## Utilizați demonstrația

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Utilizați următorul cod JS pentru a monitoriza schimbările de limbă
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parametrii interfeței

### Etichetă personalizată `<c-i18n>`

Etichete HTML personalizate, introduse în JS și utilizate în HTML.

## Managementul statusului

Utilizați `src/js/i18n.js` pentru a gestiona starea limbii globale:

- **`LANG_LI`**: matrice de listă de limbi, în formatul `[[nume, ID], ...]`.
- **`lang()`**: obțineți ID-ul actual al limbii.
- **`langCode()`**: obțineți codificarea limbii curente.
- **`langSet(id)`**: setați ID-ul limbii și notificați abonații.
- **`onLang(func)`**: Abonați-vă la modificările de limbă. Dacă limba a fost setată, apelarea va fi declanșată imediat. Returnează funcția de dezabonare.

## clasa de stil

### `.BtnC.lang`

butonul pictogramă.

### `.I18n.Lg`

Aspectul principal al ferestrei pop-up este utilizat pentru a include opțiunile de limbă.