# Blog post komponenti

- **Yillarni avtomatik guruhlash**: Maqola sanasiga ko'ra yillarni avtomatik ravishda tasniflash va ularni yil bo'yicha teskari tartibda tartiblash
- **Oddiylashtirilgan sana formati**: ko'rsatilayotganda yil prefiksini avtomatik ravishda olib tashlang va faqat oy va kunni ko'rsating
- **Markdown xulosasi va matn**: O‘rnatilgan ro‘yxatning qisqacha ko‘rinishi va bitta maqola to‘liq matnni ko‘rsatish
- **Gradient uch o‘lchamli sarlavha**: Sarlavha va chiqarilgan sanani ko‘rsatish uchun `H1Txt` ni birlashtiring

## Ko‘rsatma ishlatish

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["nom", "anstrakt", "2026-07-27"]
};

// Belgilangan maqola tanasini yuklash uchun o'tish (ro'yxatga qaytish uchun bo'sh qatorni o'tkazing)
blog.load("2026/publish");
</script>
```

## Interfeys parametrlari

### Xususiyatlari va usullari

- `blog.prefix`: Maqola havolasi prefiksini oling yoki o'rnating (masalan, `"/blog/"`)
- `blog.mdUrl`: Markdown maqola manzilini yaratish funksiyasini oling yoki sozlang `(key, lang) => url`
- `blog._`: Maqola ro'yxati metama'lumotlar lug'atini `{ [key]: [title, summary, date] }` oling yoki o'rnating
- `blog.load(key)`: Belgilangan kalitning maqola mazmunini yuklash uchun almashtiring, blog roʻyxatiga qaytish uchun boʻsh qatorni `""` oʻtkazing.