# İnterfeys dillərini dəyişmək üçün düymələr və açılan pəncərələr

- Dil seçimi üzən təbəqəni açmaq üçün düyməni basın.
- Hazırda seçilmiş dili avtomatik olaraq vurğulayın.

## Demodan istifadə edin

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Dil dəyişikliklərinə nəzarət etmək üçün aşağıdakı JS kodundan istifadə edin
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## İnterfeys parametrləri

### Fərdi teq `<c-i18n>`

JS-də təqdim edilən və HTML-də istifadə edilən xüsusi HTML teqləri.

## Statusun idarə edilməsi

Qlobal dil statusunu idarə etmək üçün `src/js/i18n.js` istifadə edin:

- **`LANG_LI`**: `[[ad, ID], ...]` formatında dil siyahısı massivi.
- **`lang()`**: Cari dil ID-sini əldə edin.
- **`langCode()`**: Cari dil kodlamasını əldə edin.
- **`langSet(id)`**: Dil identifikatorunu təyin edin və abunəçiləri xəbərdar edin.
- **`onLang(func)`**: Dil dəyişikliklərinə abunə olun. Dil qurulubsa, geri zəng dərhal işə salınacaq. Abunədən çıxmaq funksiyasını qaytarın.

## stil sinfi

### `.BtnC.lang`

ikon düyməsi.

### `.I18n.Lg`

Açılan pəncərənin əsas tərtibatı dil seçimlərini bağlamaq üçün istifadə olunur.