# Knappar och popup-fönster för att byta gränssnittsspråk

- Klicka på knappen för att visa det flytande lagret för val av språk.
- Markera automatiskt det valda språket.

## Använd demon

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Använd följande JS-kod för att övervaka språkändringar
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Gränssnittsparametrar

### Anpassad tagg `<c-i18n>`

Anpassade HTML-taggar, introducerade i JS och används i HTML.

## Statushantering

Använd `src/js/i18n.js` för att hantera global språkstatus:

- **`LANG_LI`**: Språklista, i formatet `[[namn, ID], ...]`.
- **`lang()`**: Hämta det aktuella språk-ID.
- **`langCode()`**: Hämta den aktuella språkkodningen.
- **`langSet(id)`**: Ställ in språk-ID och meddela prenumeranter.
- **`onLang(func)`**: Prenumerera på språkändringar. Om språket har ställts in utlöses återuppringningen omedelbart. Returnera avregistreringsfunktionen.

## stilklass

### `.BtnC.lang`

ikonen knappen.

### `.I18n.Lg`

Huvudlayouten för popup-fönstret används för att radbryta språkalternativen.