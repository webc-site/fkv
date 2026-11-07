# Botymau a ffenestri naid ar gyfer newid ieithoedd rhyngwyneb

- Cliciwch y botwm i agor yr haen symudol dewis iaith.
- Amlygwch yn awtomatig yr iaith a ddewiswyd ar hyn o bryd.

## Defnyddiwch y demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Defnyddiwch y cod JS canlynol i fonitro newidiadau iaith
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Paramedrau rhyngwyneb

### Tag personol `<c-i18n>`

Tagiau HTML personol, wedi'u cyflwyno yn JS a'u defnyddio yn HTML.

## Rheoli statws

Defnyddiwch `src/js/i18n.js` i reoli statws iaith byd-eang:

- **`LANG_LI`**: Arae rhestr iaith, yn y fformat `[[enw, ID], ...]`.
- **`lang()`**: Cael y ID iaith cyfredol.
- **`langCode()`**: Cael yr amgodiad iaith presennol.
- **`langSet(id)`**: Gosod yr ID iaith a hysbysu'r tanysgrifwyr.
- **`onLang(func)`**: Tanysgrifio i newidiadau iaith. Os yw'r iaith wedi'i gosod, bydd yr alwad yn ôl yn cael ei sbarduno ar unwaith. Dychwelyd y swyddogaeth dad-danysgrifio.

## dosbarth arddull

### `.BtnC.lang`

botwm eicon.

### `.I18n.Lg`

Defnyddir prif gynllun y ffenestr naid i lapio'r opsiynau iaith.