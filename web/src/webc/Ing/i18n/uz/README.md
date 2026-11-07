# Yuklash yoki ishlov berish holati ko'rsatkichi

- Kursorni kutish holatiga birlashtiring
- Qayta ishlash holatini ko'rsatish uchun to'lqinli animatsiyani ko'rsatadi
- Takroriy topshiriqlarni oldini olish uchun shaklga xiralashtirish niqobini qo'llang

## Ko‘rsatma ishlatish

```html
<!-- mustaqil yuklash ko'rsatkichi -->
<div class="Ing"></div>

<!-- Niqob bilan shakl yuklanmoqda -->
<form class="Ing">
  <input type="text" placeholder="Kirish qutisi">
  <button type="submit">topshirish</button>
</form>
```

## uslublar sinfi

### `.Ing`
- Pointer holati - `wait`.
- Pseudo element `::before` to'lqinli animatsiyani yaratadi.
- `form` elementida foydalanilganda, pseudo-element `::after` 1px xiralashtirish niqobini yaratadi, bu esa o'zaro ta'sir va matn tanlashni o'chirib qo'yadi.