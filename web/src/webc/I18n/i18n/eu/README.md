# Interfazearen hizkuntzak aldatzeko botoiak eta pop-up leihoak

- Egin klik botoian hizkuntza hautatzeko geruza mugikorra agertzeko.
- Nabarmendu automatikoki unean hautatutako hizkuntza.

## Erabili demoa

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Erabili JS kodea hau hizkuntza aldaketak kontrolatzeko
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Interfazearen parametroak

### Etiketa pertsonalizatua `<c-i18n>`

HTML etiketa pertsonalizatuak, JSn sartuak eta HTMLn erabiliak.

## Egoeraren kudeaketa

Erabili `src/js/i18n.js` hizkuntzaren egoera globala kudeatzeko:

- **`LANG_LI`**: Hizkuntzen zerrenda-matrizea, `[[izena, ID], ...]` formatuan.
- **`lang()`**: Lortu uneko hizkuntzaren IDa.
- **`langCode()`**: Lortu uneko hizkuntza-kodeketa.
- **`langSet(id)`**: ezarri hizkuntzaren IDa eta jakinarazi harpidedunei.
- **`onLang(func)`**: Harpidetu hizkuntza-aldaketetarako. Hizkuntza ezarri bada, deia berehala abiaraziko da. Itzuli harpidetza kentzeko funtzioa.

## estilo klasea

### `.BtnC.lang`

ikono botoia.

### `.I18n.Lg`

Pop-up leihoaren diseinu nagusia hizkuntza-aukerak biltzeko erabiltzen da.