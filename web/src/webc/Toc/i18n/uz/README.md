# Katalog navigatsiya komponenti

- **Responsive tortma**: Keng ekranda yon panelni ko‘rsatish, tor ekranda avtomatik ravishda yig‘ish va `c-side` tortma va sarlavhani ochish tugmasi bilan hamkorlik qilish
- **Ko'p tilli drayver**: Obyekt lug'atini yoki ko'p tilli funksiyalarni qo'llab-quvvatlaydi, tilni almashtirishga avtomatik javob beradi
- **Avtomatik quyi katalog**: Maqsadli konteynerdagi `h1`~`h6` sarlavhalarini avtomatik ravishda tahlil qilib, ikkilamchi katalogni yarating va Xesh ko‘rish oynasining aylanib o‘tishini ta’kidlang.
- **Asinxron yuklash**: Markdown matnini avtomatik ravishda asinxron tarzda olish va uni `Ing` yuklash animatsiyasini ko‘rsatuvchi kontent maydoniga kiritish uchun katalog elementini bosing.

## Ko‘rsatma ishlatish

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "komponentdan foydalanish"
});
</script>
```

## Interfeys parametrlari

### Xususiyatlari va usullari

- `toc.prefix`: Marshrutlash prefiksi qatorini oling yoki o'rnating (masalan, `"doc"`, keyingi chiziq ixtiyoriy), `README` tugmasi avtomatik ravishda bo'sh yo'lga (ya'ni, prefiksning o'zi) ko'rsatiladi.
- `toc.li`: Ko'p tilli katalog yaratish funksiyasini oling yoki o'rnating `(lang) => ({ key: title })`
- `toc.mdUrl`: Markdown URL yaratish funksiyasini oling yoki sozlang `(key, lang) => url`
- `toc.target`: Bog'langan maqsadli renderlash tugunini oling yoki o'rnating (masalan, `c-md`). Agar belgilanmagan bo'lsa, u avtomatik ravishda `c-md` / `main` / `article`ni topadi.
- `toc.load(key)`: Hujjatni qo'lda yuklash va kalitga mos keladigan katalogni kengaytirish
- `toc.sync()`: Joriy maqsadli tugundagi sarlavhani qo'lda qayta tahlil qilish, ikkilamchi katalogni sinxronlash va holatni ajratib ko'rsatish