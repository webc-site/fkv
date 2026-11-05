# Knäppercher a Pop-up Fënstere fir Interface Sproochen ze wiesselen

- Klickt op de Knäppchen fir d'Sproochauswiel schwiewend Schicht opzemaachen.
- Markéiert automatesch déi aktuell gewielte Sprooch.

## Benotzt d'Demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Benotzt de folgende JS Code fir Sproochenännerungen ze iwwerwaachen
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Interface Parameteren

### Benotzerdefinéiert Tag `<c-i18n>`

Benotzerdefinéiert HTML Tags, a JS agefouert an an HTML benotzt.

## Status Gestioun

Benotzt `src/js/i18n.js` fir de globale Sproochestatus ze managen:

- **`LANG_LI`**: Sproochelëscht Array, am Format vun `[[Numm, ID], ...]`.
- **`lang()`**: Kritt déi aktuell Sprooch-ID.
- **`langCode()`**: Kritt déi aktuell Sproochkodéierung.
- **`langSet(id)`**: Setzt d'Sprooch ID an informéiert d'Abonnenten.
- **`onLang(func)`**: Abonnéiert Iech op Sproochännerungen. Wann d'Sprooch agestallt ass, gëtt de Réckruff direkt ausgeléist. Gitt d'Abonnementfunktioun zréck.

## Stil Klass

### `.BtnC.lang`

icon Knäppchen.

### `.I18n.Lg`

Den Haapt Layout vun der Pop-up Fënster gëtt benotzt fir d'Sproochoptiounen ze wéckelen.