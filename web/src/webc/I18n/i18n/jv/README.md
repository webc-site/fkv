# Tombol lan jendhela pop-up kanggo ngoper basa antarmuka

- Klik tombol kanggo pop munggah pilihan basa ngambang lapisan.
- Nyorot basa sing dipilih kanthi otomatis.

## Gunakake demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Gunakake kode JS ing ngisor iki kanggo ngawasi owah-owahan basa
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Paramèter antarmuka

### Tag khusus `<c-i18n>`

Tag HTML khusus, dikenalake ing JS lan digunakake ing HTML.

## Manajemen status

Gunakake `src/js/i18n.js` kanggo ngatur status basa global:

- **`LANG_LI`**: Array dhaptar basa, kanthi format `[[jeneng, ID], ...]`.
- **`lang()`**: Entuk ID basa saiki.
- **`langCode()`**: Entuk enkoding basa saiki.
- **`langSet(id)`**: Setel ID basa lan ngabari pelanggan.
- **`onLang(func)`**: Langganan owah-owahan basa. Yen basa wis disetel, telpon bali bakal langsung micu. Mulihake fungsi unsubscribe.

## kelas gaya

### `.BtnC.lang`

tombol lambang.

### `.I18n.Lg`

Tata letak utama jendela pop-up digunakake kanggo mbungkus pilihan basa.