# Yopish tugmasi bilan qalqib chiquvchi qatlamni yarating va ko'rsating

- Qalqib chiquvchi qatlamni yaratish va ko'rsatish uchun funktsional chaqiruv
- O'rnatilgan yopish tugmasi, DOM-dan elementni yopish va olib tashlash uchun bosing
- Kirish maydonini fokusdan chiqarish yoki qalqib chiquvchi qatlamni yopish uchun Esc tugmasini bosing

## Ko‘rsatma ishlatish

```html
<button>Ochilish uchun bosing</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "matn";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Interfeys parametrlari

Standart eksport funksiyasi `HTMLDialogElement`ni qaytaradi va dialog elementiga `Box` va `X` uslublar sinflari qo'shilgan.

## uslublar sinfi

### `.Box.X`

`dialog` elementiga qo'llaniladi, dialog uslubini belgilaydi.

### `a.X`

`a` elementiga qo'llaniladi, yopish tugmasi uslubini belgilaydi.