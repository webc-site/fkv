# Hnappar og sprettigluggar til að skipta um tungumál viðmóts

- Smelltu á hnappinn til að skjóta upp tungumálavalinu fljótandi lag.
- Merktu sjálfkrafa valið tungumál.

## Notaðu kynninguna

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Notaðu eftirfarandi JS kóða til að fylgjast með tungumálabreytingum
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Viðmótsbreytur

### Sérsniðið merki `<c-i18n>`

Sérsniðin HTML merki, kynnt í JS og notuð í HTML.

## Stöðustjórnun

Notaðu `src/js/i18n.js` til að stjórna alþjóðlegri tungumálastöðu:

- **`LANG_LI`**: Tungumálalistafylki, á sniðinu `[[nafn, auðkenni], ...]`.
- **`lang()`**: Fáðu núverandi tungumálauðkenni.
- **`langCode()`**: Fáðu núverandi tungumálakóðun.
- **`langSet(id)`**: Stilltu tungumálakennið og láttu áskrifendur vita.
- **`onLang(func)`**: Gerast áskrifandi að tungumálabreytingum. Ef tungumálið hefur verið stillt verður svarhringingin ræst strax. Skilaðu afskráningaraðgerðinni.

## stílaflokkur

### `.BtnC.lang`

táknhnappur.

### `.I18n.Lg`

Aðalskipulag sprettigluggans er notað til að vefja tungumálamöguleikana.