# Nggawe lan nampilake lapisan popup

- Telpon fungsional kanggo nggawe lan nampilake lapisan popup
- Mbusak unsur saka DOM kanthi otomatis nalika ditutup
- Nyegah prilaku pembatalan tombol Esc

## Gunakake demo

```html
<button>Klik kanggo pop munggah</button>

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
  description.textContent = "Teks";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Paramèter antarmuka

### Default diekspor fungsi

`Box()`

- **Nilai bali**: `HTMLDialogElement`, unsur dialog kelas gaya `Box` wis ditambahake.

## kelas gaya

### `.Box`

Ditrapake menyang elemen `dialog` kanggo nyetel posisi, tengah, lan blur topeng latar mburi.

### `.Lg`

Kelas gaya sing ditrapake kanggo isi batin, nyetel werna latar mburi, sudhut bunder, bayangan lan tata letak vertikal, lan nemtokake tata letak `h3` lan `p` njero.