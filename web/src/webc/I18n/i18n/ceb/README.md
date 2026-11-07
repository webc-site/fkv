# Mga buton ug mga pop-up nga bintana para sa pagbalhin sa mga pinulongan sa interface

- I-klik ang buton aron i-pop up ang naglutaw nga layer sa pagpili sa pinulongan.
- Awtomatikong i-highlight ang gipili karon nga pinulongan.

## Gamita ang demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Gamita ang mosunod nga JS code sa pagmonitor sa mga kausaban sa pinulongan
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Mga parametro sa interface

### Custom nga tag `<c-i18n>`

Custom HTML tags, gipaila sa JS ug gigamit sa HTML.

## Pagdumala sa kahimtang

Gamita ang `src/js/i18n.js` sa pagdumala sa kahimtang sa tibuok kalibutan nga pinulongan:

- **`LANG_LI`**: Laray sa lista sa pinulongan, sa pormat nga `[[ngalan, ID], ...]`.
- **`lang()`**: Kuhaa ang kasamtangang ID sa pinulongan.
- **`langCode()`**: Kuhaa ang kasamtangan nga pag-encode sa pinulongan.
- **`langSet(id)`**: Itakda ang ID sa pinulongan ug ipahibalo ang mga subscriber.
- **`onLang(func)`**: Mag-subscribe sa mga kausaban sa pinulongan. Kon ang pinulongan gitakda na, ang callback ma-trigger dayon. Ibalik ang unsubscribe function.

## estilo nga klase

### `.BtnC.lang`

buton sa icon.

### `.I18n.Lg`

Ang nag-unang layout sa pop-up nga bintana gigamit sa pagputos sa mga kapilian sa pinulongan.