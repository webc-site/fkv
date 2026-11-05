# Matnni vaqtinchalik xotiraga nusxalash

Belgilangan elementning matnini nusxalash uchun tugmani bosing.

- Nusxalanadigan element selektorini belgilash uchun rel dan foydalanishni qo'llab-quvvatlaydi
- Belgilanmagan bo'lsa, avvalgi birodar element sukut bo'yicha ko'chiriladi.
- Ichki o'rnatilgan Pophover so'rov oynasi
- Xalqaro yordam

## Ko‘rsatma ishlatish

```html
<!-- Muayyan elementlardan nusxa ko'chiring -->
<span id="target">Bu erda nusxa ko'chiradigan matn</span>
<c-copy rel="#target"></c-copy>

<!-- Oldingi elementlar sukut bo'yicha nusxalanadi -->
<span>oldingi elementning matni</span>
<c-copy></c-copy>
```

## parametr

| mulk | tasvirlab bering |
| --- | --- |
| rel | Maqsadli element selektoridan nusxa oling. Ixtiyoriy |

## status uslubi

- `c-pophover.Done`: muvaffaqiyatli nusxalashdan so'ng so'rov qutisi uslubi