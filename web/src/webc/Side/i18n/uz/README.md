# Yon tomondan chiqib ketadigan tortma panellari

- Avtomatik ravishda ichki aylantirish tuzilmalarini yaratish
- Avtomatik ravishda yopish uchun tashqi maydonni bosing
- Yopish uchun chapga va o‘ngga suring
- Chap va o'ng yo'nalishda yon tomonga siljishning joylashishi
- Ochiq va yopiq tadbirlarni taqdim eting

## Ko‘rsatma ishlatish

```html
<button class="open-btn">Ochiq</button>

<c-side class="right">
  <div>Shkaf tarkibi</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interfeys

### usuli

- `open(el)`: Eksport funksiyasi, tortmasini ochish
- `close(el)`: Eksport funksiyasi, tortmasini yoping
- `el.open()`: misol usuli, tortmani ochish
- `el.close()`: misol usuli, tortmasini yoping

### voqea

- `open`: yoqilganda yonadi
- `close`: yopilganda yonadi

## uslublar sinfi

- `.right`: joylashuvni o'ngga suring
- `.top`: joylashishni aniqlash
- `.bottom`: joylashishni aniqlashdan tashqariga siljitish
- (Agar o'rnatilmagan bo'lsa, sukut bo'yicha chapga siljitish)
- `.open`: ochiq holat
- `.H`: yashirin holat
- `.T`: animatsiyaga o'tish holati