# Tombol dan jendela pop-up untuk mengganti bahasa antarmuka

- Klik tombol untuk memunculkan lapisan mengambang pemilihan bahasa.
- Secara otomatis menyorot bahasa yang dipilih saat ini.

## Gunakan demonya

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Gunakan kode JS berikut untuk memantau perubahan bahasa
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parameter antarmuka

### Tag khusus `<c-i18n>`

Tag HTML khusus, diperkenalkan di JS dan digunakan dalam HTML.

## Manajemen status

Gunakan `src/js/i18n.js` untuk mengelola status bahasa global:

- **`LANG_LI`**: Array daftar bahasa, dalam format `[[nama, ID], ...]`.
- **`lang()`**: Dapatkan ID bahasa saat ini.
- **`langCode()`**: Dapatkan pengkodean bahasa saat ini.
- **`langSet(id)`**: Tetapkan ID bahasa dan beri tahu pelanggan.
- **`onLang(func)`**: Berlangganan perubahan bahasa. Jika bahasa telah disetel, panggilan balik akan segera dipicu. Kembalikan fungsi berhenti berlangganan.

## kelas gaya

### `.BtnC.lang`

tombol ikon.

### `.I18n.Lg`

Tata letak utama jendela pop-up digunakan untuk membungkus opsi bahasa.