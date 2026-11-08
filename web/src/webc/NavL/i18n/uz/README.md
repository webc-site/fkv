# Navigatsiya menyusini va qaytish tugmasini avtomatik ravishda almashtirish

- Navigatsiya menyusini yaratish va tugma tuzilmalarini qaytarish uchun bolalar elementlarini avtomatik ravishda oʻrash
- Marshrutlash tarixini avtomatik ravishda kuzatib boring va oldingi qaytish yo'lini hisoblang
- Marshrut menyu bandiga to‘g‘ri kelganda navigatsiya menyusini ko‘rsating, aks holda orqaga tugmani ko‘rsating

## Ko‘rsatma ishlatish

```html
<c-nav-l>
  <a href="/">oldingi sahifa</a>
  <a href="/doc">hujjat</a>
</c-nav-l>
```

## uslublar sinfi

- `B`: qaytish tugmasi holatini faollashtiradi. Orqaga tugmani ko'rsating va navigatsiya menyusini yashiring. Marshrutlash asosida komponent ichida avtomatik ravishda almashtiriladi.