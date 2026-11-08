# Nggawe lan nampilake lapisan popup kanthi tombol cedhak

- Telpon fungsional kanggo nggawe lan nampilake lapisan popup
- Tombol cedhak sing dibangun, klik kanggo nutup lan mbusak unsur saka DOM
- Pencet Esc kanggo defocus kothak input utawa nutup lapisan popup

## Gunakake demo

```html
<button>Klik kanggo pop munggah</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "teks";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Paramèter antarmuka

Fungsi ekspor standar ngasilake `HTMLDialogElement`, lan kelas gaya `Box` lan `X` wis ditambahake menyang unsur dialog.

## kelas gaya

### `.Box.X`

Ditrapake menyang `dialog` unsur, nemtokake gaya dialog.

### `a.X`

Ditrapake menyang elemen `a`, nemtokake gaya tombol cedhak.