# Qalqib chiquvchi qatlamni yarating va ko'rsating

- Qalqib chiquvchi qatlamni yaratish va ko'rsatish uchun funktsional chaqiruv
- Yopiq bo'lganda DOM dan elementlarni avtomatik ravishda olib tashlang
- Esc kalitini bekor qilish xatti-harakatlarini oldini olish

## Ko‘rsatma ishlatish

```html
<button>Ochilish uchun bosing</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Sarlavha";
  description.textContent = "Matn";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Interfeys parametrlari

### Standart eksport qilingan funksiyalar

`Box()`

- **Qaytish qiymati**: `HTMLDialogElement`, `Box` uslub sinfining dialog elementi qo'shildi.

## uslublar sinfi

### `.Box`

Joylashtirish, markazlashtirish va fonni xiralashtirishni o‘rnatish uchun `dialog` elementiga qo‘llaniladi.

### `.Lg`

Ichki tarkibga qo'llaniladigan uslublar sinfi, fon rangi, yumaloq burchaklar, soya va vertikal tartibni o'rnatish va ichki `h3` va `p` tartibini belgilash.