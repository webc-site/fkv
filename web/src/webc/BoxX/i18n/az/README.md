# Bağlama düyməsi ilə pop-up qat yaradın və göstərin

- Popup təbəqəni yaratmaq və göstərmək üçün funksional çağırış
- Daxili bağlama düyməsi, elementi DOM-dan bağlamaq və silmək üçün klikləyin
- Giriş qutusunu defokus etmək və ya açılan təbəqəni bağlamaq üçün Esc düyməsini basın

## Demodan istifadə edin

```html
<button>Açmaq üçün klikləyin</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "mətn";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## İnterfeys parametrləri

Defolt ixrac funksiyası `HTMLDialogElement` qaytarır və `Box` və `X` üslub sinifləri dialoq elementinə əlavə edilib.

## stil sinfi

### `.Box.X`

`dialog` elementinə tətbiq olunur, dialoq üslubunu müəyyən edir.

### `a.X`

`a` elementinə tətbiq edilir, bağlama düyməsinin üslubunu müəyyən edir.