# Buat dan paparkan lapisan pop timbul dengan butang tutup

- Panggilan berfungsi untuk mencipta dan memaparkan lapisan pop timbul
- Butang tutup terbina dalam, klik untuk menutup dan mengalih keluar elemen daripada DOM
- Tekan Esc untuk menyahfokus kotak input atau menutup lapisan pop timbul

## Gunakan demo

```html
<button>Klik untuk muncul</button>

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


## Parameter antara muka

Fungsi eksport lalai mengembalikan `HTMLDialogElement` dan kelas gaya `Box` dan `X` telah ditambahkan pada elemen dialog.

## kelas gaya

### `.Box.X`

Digunakan pada elemen `dialog`, menentukan gaya dialog.

### `a.X`

Digunakan pada elemen `a`, menentukan gaya butang tutup.