# Turli xil ekran o'lchamlariga moslashtirilgan navigatsiya menyusi

- Ish stoli: havolalar gorizontal plitkalarda ko'rsatiladi
- Mobil versiya: Menyuga yig‘ish tugmasi, yon panel tortmasidan tashqariga surish uchun bosing
- Yo'nalishni faollashtirish: joriy yo'l va xesh qiymatini avtomatik ravishda solishtiring va mos keladigan havolaga faollashtirish holati sinfi nomini qo'shing `A`
- Avtomatik yopish: Mobil terminal havolani yoki yon panelning tashqi fonini bosganda, yon panel avtomatik ravishda yopiladi.

## foydalanish

```html
<c-nav-menu>
  <a href="/">oldingi sahifa</a>
  <a href="/doc">hujjat</a>
  <a href="/about">haqida</a>
</c-nav-menu>
```

## tasvirlab bering

Komponent ishga tushirilganda, agar pastki elementda `c-side` bo'lmasa, mobil menyu tugmasi va `c-side` yon panel avtomatik ravishda yaratiladi va barcha `a` teglar yon panelga ko'chiriladi.

## uslublar sinfi

- **A**: Faollashtirilgan havola avtomatik ravishda sinf nomini `A` qo'shadi