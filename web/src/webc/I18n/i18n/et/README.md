# Nupud ja hüpikaknad liidese keelte vahetamiseks

- Klõpsake nuppu, et avada keelevaliku ujuvkiht.
- Tõsta praegu valitud keel automaatselt esile.

## Kasutage demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Keelemuutuste jälgimiseks kasutage järgmist JS-koodi
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Liidese parameetrid

### Kohandatud märgend `<c-i18n>`

Kohandatud HTML-i sildid, mis on kasutusele võetud JS-is ja mida kasutatakse HTML-is.

## Staatuse juhtimine

Globaalse keele oleku haldamiseks kasutage `src/js/i18n.js`:

- **`LANG_LI`**: keelte loendi massiiv, vormingus `[[nimi, ID], ...]`.
- **`lang()`**: hankige praegune keele ID.
- **`langCode()`**: hankige praegune keelekodeering.
- **`langSet(id)`**: määrake keele ID ja teavitage tellijaid.
- **`onLang(func)`**: tellige keelemuudatused. Kui keel on määratud, käivitatakse koheselt tagasihelistamine. Taasta tellimuse tühistamise funktsioon.

## stiiliklass

### `.BtnC.lang`

ikooni nupp.

### `.I18n.Lg`

Hüpikakna põhipaigutust kasutatakse keelevalikute pakkimiseks.