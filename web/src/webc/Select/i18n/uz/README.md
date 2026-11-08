# Ochiladigan tanlov, kenglik joriy elementga moslashadi

Mahalliy `select` kengligi sukut bo'yicha eng uzun variant matni bilan belgilanadi. Qisqa variantlarni almashtirishda juda ko'p bo'sh joy bor, bu tashqi ko'rinishga ta'sir qiladi.

Ushbu komponent `b` tegi ustidagi mahalliy `select`ni shaffof tarzda qamrab oladi, bu esa kenglikning hozirda tanlangan matn uzunligiga aniq moslashishiga imkon beradi.

- Mahalliy `select` ochiladigan menyu uslublarini va barcha terminallarda maxsus imkoniyatlarni qo'llab-quvvatlang
- Klaviaturada toʻliq navigatsiya va yoʻnalish tugmachalarini almashtirish kabi mahalliy interaktiv operatsiyalarni qoʻllab-quvvatlaydi.
- Ortiqcha bo'sh joyni yo'qotish uchun kenglikni joriy variantga moslang

## Ko‘rsatma ishlatish

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```