# Sahifaning chetida qalqib chiquvchi oyna

- **Funksiya chaqiruvi**: Oddiy va xatolik haqidagi tezkor interfeyslarni taqdim etadi
- **Avtomatik stek**: Bir nechta maslahatlar oraliq va stekni vertikal ravishda avtomatik ravishda hisoblab chiqadi
- **Rejalashtirilgan va qo'lda o'chirish**: Rejalashtirilgan va qo'lda o'chirishni qo'llab-quvvatlaydi

## Ko‘rsatma ishlatish

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Tezkor tarkib";
});

toastErr((el) => {
  el.textContent = "Xato tarkibi";
});

toast(
  (el) => {
    el.textContent = "99 soniyadan keyin avtomatik ravishda yopish";
  },
  99
);
```

## Interfeys parametrlari

### toast(render, timeout)

Ko'rsatmalar yaratish va ko'rsatish.

- `render`: `(el) => void`, qayta qo'ng'iroq qilish, `el` - taklif oynasi elementi.
- `timeout`: `Number`, vaqt tugashi soniyalari. Standart 9. Avtomatik o'chmaslik uchun 0 ga o'rnating.
- Qaytish qiymati: so'rov qutisi elementi. Qo'llab-quvvatlaydi `el.close()` o'chirilgan.

### toastErr(render, timeout)

Xato xabarlarini yaratish va ko'rsatish. Parametrlar `toast` bilan bir xil, `.ERR` uslublar sinfiga ega.

## uslublar sinfi

- `.Toast`: So'rov qutisi asosiy klassi.
- `.ERR`: Xato holati.
- `.animated` / `.fadeInR` / `.fadeOutR`: jonlantirilgan o'tish.
- `.x`: Yopish tugmasi.