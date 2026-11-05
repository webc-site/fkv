# Ochiladigan tanlovga ega tab sahifasi

- Yorliqda moslashtirilgan kenglik ochiladigan tanlash oynasini (`<c-select>`) joylashtirishni qo'llab-quvvatlaydi, bu faqat joriy varaq sahifasi faollashtirilganda ko'rsatiladi.
- Strukturaviy spetsifikatsiya: Do'st element yorliqli navigatsiya sifatida `nav > a` dan foydalanadi va to'g'ridan-to'g'ri pastki element `b` kontent maydoni sifatida xizmat qiladi.
- `a[value]` tegini, `a`ni `c-select` (guruhning qiymat atributi va tanlangan parametr qiymati asosida) va `b[slot]` kontent tuguniga avtomatik bog‘laydi.
- Hozirda faollashtirilgan elementni `key` atributi orqali avtomatik ravishda `localStorage` ga saqlashni qo‘llab-quvvatlaydi.
- `key + '/' + tab` yordamida har bir guruh uchun tarixiy sub-tanlovlarni `localStorage` ga avtomatik saqlash va tiklashni qo'llab-quvvatlaydi.
- `value` atributi orqali faol varaqni belgilashni qo'llab-quvvatlaydi.

## Ko‘rsatma ishlatish

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>til</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>tizimi</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript - bu skript tili</b>
    <b slot="rs">Rust - bu tizim darajasidagi dasturlash tili</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS - bu Unix-ga asoslangan grafik operatsion tizim</b>
    <b slot="win">Windows - bu Microsoft tomonidan ishlab chiqilgan operatsion tizim</b>
  </b>
</c-tab-select>
```

## mulk

- `key`: localStorage barqarorligi uchun ishlatiladigan kalit nomi
- `value`: hozirda tanlangan qiymat

## Maxsus tadbirlar

- `change`: Yorliqlarni almashtirishda ishga tushiriladi, `e.value` hozirda faollashtirilgan parametrning `value` qiymatidir.