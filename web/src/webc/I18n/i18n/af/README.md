# Knoppies en opspringvensters om koppelvlaktale te verander

- Klik op die knoppie om die taalkeuse drywende laag op te roep.
- Merk outomaties die taal wat tans gekies is.

## Gebruik die demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Gebruik die volgende JS-kode om taalveranderinge te monitor
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Interface parameters

### Gepasmaakte merker `<c-i18n>`

Gepasmaakte HTML-etikette, bekendgestel in JS en gebruik in HTML.

## Status bestuur

Gebruik `src/js/i18n.js` om globale taalstatus te bestuur:

- **`LANG_LI`**: Taallys-skikking, in die formaat van `[[naam, ID], ...]`.
- **`lang()`**: Kry die huidige taal-ID.
- **`langCode()`**: Kry die huidige taalkodering.
- **`langSet(id)`**: Stel die taal-ID en stel intekenare in kennis.
- **`onLang(func)`**: Teken in op taalveranderings. As die taal gestel is, sal die terugbel onmiddellik geaktiveer word. Keer die uittekenfunksie terug.

## styl klas

### `.BtnC.lang`

ikoon knoppie.

### `.I18n.Lg`

Die hoofuitleg van die opspringvenster word gebruik om die taalopsies te omvou.