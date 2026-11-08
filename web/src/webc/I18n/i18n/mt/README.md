# Buttuni u twieqi pop-up biex taqleb il-lingwi tal-interface

- Ikklikkja l-buttuna biex tfaċċa s-saff galleġġjanti tal-għażla tal-lingwa.
- Enfasizza awtomatikament il-lingwa magħżula bħalissa.

## Uża d-demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Uża l-kodiċi JS li ġej biex tissorvelja l-bidliet fil-lingwa
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parametri tal-interface

### Tikketta personalizzata `<c-i18n>`

Tikketti HTML personalizzati, introdotti f'JS u użati f'HTML.

## Ġestjoni tal-istatus

Uża `src/js/i18n.js` biex timmaniġġja l-istatus tal-lingwa globali:

- **`LANG_LI`**: Array tal-lista tal-lingwi, fil-format ta' `[[isem, ID], ...]`.
- **`lang()`**: Ikseb l-ID tal-lingwa attwali.
- **`langCode()`**: Ikseb il-kodifikazzjoni tal-lingwa attwali.
- **`langSet(id)`**: Issettja l-ID tal-lingwa u avża lill-abbonati.
- **`onLang(func)`**: Abbona għal bidliet fil-lingwa. Jekk il-lingwa ġiet issettjata, is-sejħa lura tiġi attivata immedjatament. Irritorna l-funzjoni unsubscribe.

## klassi tal-istil

### `.BtnC.lang`

buttuna tal-ikona.

### `.I18n.Lg`

It-tqassim prinċipali tat-tieqa pop-up jintuża biex ikebbeb l-għażliet tal-lingwa.