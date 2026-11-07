# Pilihan gulung mudhun, jembaré cocog karo item saiki

Jembar `select` asli ditemtokake dening teks pilihan paling dawa minangka standar. Nalika ngoper opsi cendhak, akeh banget spasi putih, sing mengaruhi tampilan.

Komponen iki kanthi transparan nutupi `select` asli ing ndhuwur tag `b`, saéngga jembaré bisa adaptasi kanthi akurat karo dawa teks sing saiki dipilih.

- Njaga gaya menu gulung mudhun `select` asli lan dhukungan aksesibilitas ing kabeh terminal
- Ndhukung operasi interaktif asli kayata navigasi Tab keyboard lengkap lan ngoper tombol arah.
- Atur jembaré kanggo pilihan saiki kanggo ngilangi keluwihan spasi putih

## Gunakake demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```