# O'qish tajribasi uchun optimallashtirilgan tipografiya uslublari

- **Asosiy matn terish**: Sarlavhalar, paragraflar, qalin, kursiv, chizilgan, satr kodlari, rasmlar va ajratuvchi chiziqlarga moslash.
- **Havola animatsiyasi**: kursorni o‘tayotganda tagiga chizilgan animatsiyani ko‘rsatish.
- **Loyihalar roʻyxati**: tartiblanmagan roʻyxatlar oʻqlardan foydalanadi, tartiblangan roʻyxatlar yaxlitlangan fon seriya raqamlaridan foydalanadi va koʻp darajali joylashtirishni qoʻllab-quvvatlaydi.
- **Vazifalar roʻyxati**: Belgilash qutisi uslubini taqdim etadi, vazifa va tugallangan holatni qoʻllab-quvvatlaydi.
- **Ogohlantirish maslahatlari**: Besh turdagi ogohlantirish qutilarini qo'llab-quvvatlaydi: Eslatma, Maslahat, Muhim, Ogohlantirish va Diqqat.
- **Jadvallar va kodlar**: Jadval satriga kursorni ta'kidlashni va soyalar va gradient fonli kod bloklarini ta'minlaydi.

## Ko‘rsatma ishlatish

```html
<div class="Md">
  <h1>sarlavha</h1>
  <p>Paragraf matni va <a href="#">havola</a></p>
  <hr>
  <h2>Ikkinchi darajali sarlavha</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Vazifa elementi</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Eslatma mazmuni</p>
  </blockquote>
</div>
```

## uslublar sinfi

Uslublar HTML elementlariga `.Md` konteyner ostida qo'llaniladi.

### Ogohlantirish qutisi uslubi

Ogohlantirish oynasini almashtirish uchun `blockquote` elementiga quyidagi sinf nomini `.q` sinf nomi bilan qo'shing:

- `note`: Eslatmalar
- `tip`: Maslahat
- `important`: Muhim
- `warning`: ogohlantirish
- `caution`: Eslatma