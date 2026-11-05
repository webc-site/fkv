# Knapper og popup-vinduer for å bytte grensesnittspråk

- Klikk på knappen for å åpne det flytende laget for språkvalg.
- Merk automatisk det valgte språket.

## Bruk demoen

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Bruk følgende JS-kode for å overvåke språkendringer
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Grensesnittparametere

### Egendefinert tag `<c-i18n>`

Egendefinerte HTML-tagger, introdusert i JS og brukt i HTML.

## Statusbehandling

Bruk `src/js/i18n.js` for å administrere global språkstatus:

- **`LANG_LI`**: Språklistearray, i formatet `[[navn, ID], ...]`.
- **`lang()`**: Få gjeldende språk-ID.
- **`langCode()`**: Få gjeldende språkkoding.
- **`langSet(id)`**: Angi språk-ID og varsle abonnenter.
- **`onLang(func)`**: Abonner på språkendringer. Hvis språket er innstilt, vil tilbakeringingen bli utløst umiddelbart. Returner avmeldingsfunksjonen.

## stilklasse

### `.BtnC.lang`

ikonknappen.

### `.I18n.Lg`

Hovedoppsettet til popup-vinduet brukes til å omslutte språkalternativene.