# suzuvchi pufak so'rovi

Sichqoncha sichqonchani ko'tarilganda yoki element faollashtirilganda sichqonchani ko'tarish uchun asboblar maslahati ma'lumotlarini ko'rsatish.

- Mahalliy veb-komponentga asoslangan, engil va yuqori unumdor
- To'rt yo'nalishda moslashuvchan fazoviy joylashishni qo'llab-quvvatlaydi (sinf = "tepa/pastki/chap/o'ng")
- Unda qora muzli shisha teksturali gradient fon va oq belgilardan foydalaniladi, ularda nozik jismoniy qirralarning konturi diqqatga sazovordir.
- O'rnatilgan joylashuvga moslashtirilgan hisoblash, ko'rish oynasidagi qolgan bo'sh joyga qarab qalqib chiquvchi yo'nalishni avtomatik ravishda sozlaydi
- Suzuvchi taklif tarkibini ko'rsatish uchun `slot="pop"` dan foydalaning

## Ko‘rsatma ishlatish

```html
<!-- Yo'nalishni avtomatik hisoblash (tavsiya etiladi, sinfni e'lon qilish shart emas) -->
<c-pophover>
  <button>avtomatik</button>
  <div slot="pop">Bu pufakchali maslahat</div>
</c-pophover>

<!-- qat'iy yo'nalish -->
<c-pophover class="left">
  <button>Chapga</button>
  <div slot="pop">Bu pufakchali maslahat</div>
</c-pophover>

<c-pophover class="right">
  <button>to'g'ri</button>
  <div slot="pop">Bu pufakchali maslahat</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Pastga</button>
  <div slot="pop">Bu pufakchali maslahat</div>
</c-pophover>

<c-pophover class="top">
  <button>ustun</button>
  <div slot="pop">Bu pufakchali maslahat</div>
</c-pophover>
```

## CSS o'zgaruvchilari

Maxsus uslublar quyidagi CSS o'zgaruvchilari orqali qo'llab-quvvatlanadi:

| o'zgaruvchan nomi | standart qiymat | tasvirlab bering |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Qabariq asboblar maslahati va tetiklash elementi orasidagi bo'shliq |
| `--pophover-margin` | `24px` | Moslashuvchan joylashishni aniqlashni hisoblashda Viewport chekka xavfsizlik chegarasi |
| `--pophover-bg-top` | `#18181ce0` | Pufakcha gradient fon ustki rangi |
| `--pophover-bg-mid` | `#121215e5` | Pufakchaning chap va o'ng tomonidagi o'qlarning fon rangi |
| `--pophover-bg-bottom` | `#0c0c0feb` | Pufakcha gradient fonining pastki va yuqori va pastki yon strelkalarining rangi |

## Belgilar uchun maslahat uslubi (`.Ico`)

Sof ikonka tipidagi suzuvchi soʻrov tugmalari uchun (masalan, “Tahrirlash”, “Oʻchirish” va h.k.) tashqi konteyner yoki komponentning oʻziga `.Ico` sinf nomini qoʻshish orqali birlashtirilgan belgi va kursor uslubini qoʻllashingiz mumkin.

### Strukturaga misol

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">tahrirlash</div>
  </c-pophover>
</b>
```

Asosiy tartib, oʻlcham (18x18px) va kursor rangidagi oʻzgarishlar `.Ico > c-pophover > a` da birlashtirildi. Ishlab chiquvchilar faqat tegishli belgi faylini alohida ko'rsatishlari kerak, masalan:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```