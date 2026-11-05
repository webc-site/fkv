# Muzli shisha fon va chekka ta'kidlash effekti

Yarim shaffof hoshiya va ichki soya mikro-relefli qirralarning yorqinligini yaratadi.
Tugma va kirish qutisi komponentlarining asosiy uslubga bog'liqligi sifatida.

## Ko‘rsatma ishlatish

```html
<main class="Lg">
  <article>
    <h3>sarlavha</h3>
    <pre>Tarkib matni</pre>
  </article>
</main>
```

## uslublar sinfi

### `.Lg`
Kartaning asosiy uslubi. `backdrop-filter` xiralashtirish, `border` mikro-bo‘rttirilgan hoshiya va `box-shadow` ichki soyani qo‘llang. Kursorda `svg/glass.svg#h` filtrini qo'llang.

### `main.Lg`
Sahifaning asosiy blokli konteyneri. Yumaloq burchaklar `24px` sezgir to'ldirish bilan (`16px` kengligi `800px` dan kichik bo'lsa, `24px` aks holda).

### `.Btn`, `.BtnC`, `.Input`
Asosiy komponent sinf nomi. Ushbu uslubni kiritganingizdan so'ng, siz avtomatik ravishda moslashuvchan tartibni, ierarxik munosabatlarni va muzli shishaning asosiy xususiyatlarini olasiz.