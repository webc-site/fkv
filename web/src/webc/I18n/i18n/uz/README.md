# Interfeys tillarini almashtirish uchun tugmalar va qalqib chiquvchi oynalar

- Til tanlash suzuvchi qatlamni ochish uchun tugmani bosing.
- Tanlangan tilni avtomatik ravishda ajratib ko'rsatish.

## Ko‘rsatma ishlatish

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Til o'zgarishlarini kuzatish uchun quyidagi JS kodidan foydalaning
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Interfeys parametrlari

### Maxsus teg `<c-i18n>`

JS-da kiritilgan va HTML-da ishlatiladigan maxsus HTML teglari.

## Status boshqaruvi

Global til holatini boshqarish uchun `src/js/i18n.js` dan foydalaning:

- **`LANG_LI`**: `[[name, ID], ...]` formatida tillar ro‘yxati massivi.
- **`lang()`**: Joriy til identifikatorini oling.
- **`langCode()`**: Joriy til kodlashini oling.
- **`langSet(id)`**: Til identifikatorini o'rnating va obunachilarni xabardor qiling.
- **`onLang(func)`**: Til o‘zgarishlariga obuna bo‘ling. Agar til o'rnatilgan bo'lsa, qayta qo'ng'iroq darhol ishga tushadi. Obunani bekor qilish funksiyasini qaytaring.

## uslublar sinfi

### `.BtnC.lang`

belgi tugmasi.

### `.I18n.Lg`

Qalqib chiquvchi oynaning asosiy tartibi til parametrlarini o'rash uchun ishlatiladi.