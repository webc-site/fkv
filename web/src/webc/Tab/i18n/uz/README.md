# O'zgaruvchan yorliqlar

- Yorliqning tanlangan holati quyidagi kontent maydoni bilan muammosiz birlashtirilgan.
- Strukturaviy spetsifikatsiyalar: Toʻliq elementlar yorliq navigatsiyasi sifatida `nav > a` dan foydalanish uchun oʻrnatiladi va toʻgʻridan-toʻgʻri yordamchi element `b` kontent maydoni sifatida ishlatiladi.
- `a[value]` tegini `b[slot]` kontent tuguniga avtomatik bog‘lash.
- Joriy faol sahifani `key` atributi orqali avtomatik ravishda `localStorage` ga saqlashni qo'llab-quvvatlaydi.
- `value` atributi orqali faollashtirilgan yorliq sahifasini belgilashni qo'llab-quvvatlaydi (agar kesh va `value` bo'lmasa, sukut bo'yicha `class="A"`li teg yoki birinchi teg tanlanadi).
- Yengil engil DOMni amalga oshirish, Shadow DOM to'siqlari yo'q, uslubni sozlashda yuqori darajadagi erkinlik.

## Ko‘rsatma ishlatish

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">olma</a>
    <a value="banana">banan</a>
    <a value="orange">mandarin</a>
  </nav>
  <b slot="apple">Olma to'yimli meva hisoblanadi</b>
  <b slot="banana">Banan tropik meva hisoblanadi</b>
  <b slot="orange">Apelsinlar S vitaminiga boy</b>
</c-tab>
```

## mulk

- `key`: localStorage barqarorligi uchun ishlatiladigan kalit nomi
- `value`: hozirda tanlangan qiymat

## Maxsus tadbirlar

- `change`: Yorliq almashtirilganda ishga tushadi, `e.value` - hozirda faollashtirilgan tegning `value` qiymati.