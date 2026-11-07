# Butang dan tetingkap timbul untuk menukar bahasa antara muka

- Klik butang untuk memunculkan lapisan terapung pemilihan bahasa.
- Serlahkan secara automatik bahasa yang dipilih pada masa ini.

## Gunakan demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Gunakan kod JS berikut untuk memantau perubahan bahasa
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parameter antara muka

### Teg tersuai `<c-i18n>`

Teg HTML tersuai, diperkenalkan dalam JS dan digunakan dalam HTML.

## Pengurusan status

Gunakan `src/js/i18n.js` untuk mengurus status bahasa global:

- **`LANG_LI`**: Tatasusunan senarai bahasa, dalam format `[[name, ID], ...]`.
- **`lang()`**: Dapatkan ID bahasa semasa.
- **`langCode()`**: Dapatkan pengekodan bahasa semasa.
- **`langSet(id)`**: Tetapkan ID bahasa dan maklumkan pelanggan.
- **`onLang(func)`**: Langgan perubahan bahasa. Jika bahasa telah ditetapkan, panggilan balik akan dicetuskan serta-merta. Kembalikan fungsi berhenti melanggan.

## kelas gaya

### `.BtnC.lang`

butang ikon.

### `.I18n.Lg`

Reka letak utama tetingkap timbul digunakan untuk membungkus pilihan bahasa.