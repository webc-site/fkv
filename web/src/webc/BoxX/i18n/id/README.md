# Buat dan tampilkan lapisan popup dengan tombol tutup

- Panggilan fungsional untuk membuat dan menampilkan lapisan popup
- Tombol tutup bawaan, klik untuk menutup dan menghapus elemen dari DOM
- Tekan Esc untuk mendefokus kotak input atau menutup lapisan popup

## Gunakan demonya

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


## Parameter antarmuka

Fungsi ekspor default mengembalikan `HTMLDialogElement`, dan kelas gaya `Box` dan `X` telah ditambahkan ke elemen dialog.

## kelas gaya

### `.Box.X`

Diterapkan pada elemen `dialog`, menentukan gaya dialog.

### `a.X`

Diterapkan pada elemen `a`, menentukan gaya tombol tutup.