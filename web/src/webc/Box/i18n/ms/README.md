# Buat dan paparkan lapisan pop timbul

- Panggilan berfungsi untuk mencipta dan memaparkan lapisan pop timbul
- Alih keluar elemen secara automatik daripada DOM apabila ditutup
- Cegah kelakuan pembatalan kunci Esc

## Gunakan demo

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
  title.textContent = "Tajuk";
  description.textContent = "Teks";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parameter antara muka

### Fungsi eksport lalai

`Box()`

- **Nilai pulangan**: `HTMLDialogElement`, elemen dialog kelas gaya `Box` telah ditambahkan.

## kelas gaya

### `.Box`

Digunakan pada elemen `dialog` untuk menetapkan kedudukan, pemusatan dan kabur topeng latar belakang.

### `.Lg`

Kelas gaya digunakan pada kandungan dalam, menetapkan warna latar belakang, bucu bulat, bayang-bayang dan susun atur menegak dan mentakrifkan reka letak `h3` dan `p` dalam.