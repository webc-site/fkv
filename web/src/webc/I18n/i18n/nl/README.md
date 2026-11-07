# Knoppen en pop-upvensters voor het wisselen van interfacetalen

- Klik op de knop om de zwevende laag voor taalselectie te openen.
- Markeer automatisch de momenteel geselecteerde taal.

## Gebruik de demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Gebruik de volgende JS-code om taalwijzigingen te monitoren
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Interfaceparameters

### Aangepaste tag `<c-i18n>`

Aangepaste HTML-tags, geïntroduceerd in JS en gebruikt in HTML.

## Statusbeheer

Gebruik `src/js/i18n.js` om de algemene taalstatus te beheren:

- **`LANG_LI`**: Taallijstarray, in de indeling `[[naam, ID], ...]`.
- **`lang()`**: Haal de huidige taal-ID op.
- **`langCode()`**: Haal de huidige taalcodering op.
- **`langSet(id)`**: Stel de taal-ID in en breng abonnees op de hoogte.
- **`onLang(func)`**: Abonneer u op taalwijzigingen. Als de taal is ingesteld, wordt de terugbelactie onmiddellijk geactiveerd. Retourneer de afmeldfunctie.

## stijl klasse

### `.BtnC.lang`

pictogram knop.

### `.I18n.Lg`

De hoofdindeling van het pop-upvenster wordt gebruikt om de taalopties weer te geven.