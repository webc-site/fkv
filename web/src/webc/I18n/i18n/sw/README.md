# Vifungo na madirisha ibukizi kwa kubadili lugha za kiolesura

- Bofya kitufe ili kuibua safu inayoelea ya uteuzi wa lugha.
- Angazia kiotomatiki lugha iliyochaguliwa kwa sasa.

## Tumia onyesho

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Tumia nambari ifuatayo ya JS kufuatilia mabadiliko ya lugha
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Vigezo vya interface

### Lebo maalum `<c-i18n>`

Lebo maalum za HTML, zilizoletwa katika JS na kutumika katika HTML.

## Usimamizi wa hali

Tumia `src/js/i18n.js` kudhibiti hali ya lugha ulimwenguni:

- **`LANG_LI`**: Orodha ya orodha ya lugha, katika umbizo la `[[jina, ID], ...]`.
- **`lang()`**: Pata kitambulisho cha sasa cha lugha.
- **`langCode()`**: Pata usimbaji wa sasa wa lugha.
- **`langSet(id)`**: Weka kitambulisho cha lugha na uwajulishe waliojisajili.
- **`onLang(func)`**: Jisajili kwa mabadiliko ya lugha. Ikiwa lugha imewekwa, upigaji simu utaanzishwa mara moja. Rejesha kipengele cha kujiondoa.

## darasa la mtindo

### `.BtnC.lang`

kitufe cha ikoni.

### `.I18n.Lg`

Mpangilio mkuu wa dirisha ibukizi hutumiwa kufunga chaguzi za lugha.