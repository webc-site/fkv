# Membuat dan menampilkan lapisan popup

- Panggilan fungsional untuk membuat dan menampilkan lapisan popup
- Hapus elemen dari DOM secara otomatis saat ditutup
- Cegah perilaku pembatalan kunci Esc

## Gunakan demonya

```html
<button>Klik untuk muncul</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Judul";
  deskripsi.textContent = "Teks";
  button.className = "Btn Main";
  tombol.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parameter antarmuka

### Fungsi yang diekspor secara default

`Box()`

- **Nilai kembalian**: `HTMLDialogElement`, elemen dialog kelas gaya `Box` telah ditambahkan.

## kelas gaya

### `.Box`

Berlaku pada elemen `dialog` untuk menyetel pemosisian, pemusatan, dan keburaman masker latar belakang.

### `.Lg`

Kelas gaya yang diterapkan pada konten bagian dalam, mengatur warna latar belakang, sudut membulat, bayangan dan tata letak vertikal, serta menentukan tata letak `h3` dan `p` bagian dalam.