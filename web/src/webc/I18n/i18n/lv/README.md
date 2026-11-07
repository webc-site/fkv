# Pogas un uznirstošie logi interfeisa valodu pārslēgšanai

- Noklikšķiniet uz pogas, lai atvērtu valodas atlases peldošo slāni.
- Automātiski iezīmējiet pašlaik atlasīto valodu.

## Izmantojiet demonstrāciju

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Izmantojiet šo JS kodu, lai pārraudzītu valodas izmaiņas
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Interfeisa parametri

### Pielāgots tags `<c-i18n>`

Pielāgoti HTML tagi, kas ieviesti JS un tiek izmantoti HTML.

## Statusa vadība

Izmantojiet `src/js/i18n.js`, lai pārvaldītu globālās valodas statusu:

- **`LANG_LI`**: valodu saraksta masīvs šādā formātā: `[[vārds, ID], ...]`.
- **`lang()`**: iegūstiet pašreizējās valodas ID.
- **`langCode()`**: iegūstiet pašreizējās valodas kodējumu.
- **`langSet(id)`**: iestatiet valodas ID un paziņojiet abonentiem.
- **`onLang(func)`**: abonējiet valodas izmaiņas. Ja valoda ir iestatīta, atzvanīšana tiks aktivizēta nekavējoties. Atgriezt abonēšanas atcelšanas funkciju.

## stila klase

### `.BtnC.lang`

ikonas poga.

### `.I18n.Lg`

Uznirstošā loga galvenais izkārtojums tiek izmantots, lai aplauztu valodas opcijas.