# Tab boleh tukar

- Keadaan label yang dipilih disepadukan dengan lancar dengan kawasan kandungan di bawah.
- Spesifikasi struktur: Elemen anak ditetapkan untuk menggunakan `nav > a` sebagai navigasi label dan elemen anak langsung `b` digunakan sebagai kawasan kandungan.
- Mengaitkan teg `a[value]` secara automatik dengan nod kandungan `b[slot]`.
- Menyokong secara automatik mengekalkan halaman aktif semasa ke `localStorage` melalui atribut `key`.
- Menyokong menentukan halaman tab yang diaktifkan melalui atribut `value` (jika tiada cache dan `value`, teg dengan `class="A"` atau teg pertama akan dipilih secara lalai).
- Pelaksanaan DOM Ringan Ringan, tiada halangan Shadow DOM, tahap kebebasan yang tinggi dalam penyesuaian gaya.

## Gunakan demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">epal</a>
    <a value="banana">pisang</a>
    <a value="orange">tangerine</a>
  </nav>
  <b slot="apple">Epal adalah buah yang berkhasiat</b>
  <b slot="banana">Pisang ialah buah tropika</b>
  <b slot="orange">Oren kaya dengan vitamin C</b>
</c-tab>
```

## harta benda

- `key`: nama kunci yang digunakan untuk kegigihan localStorage
- `value`: nilai yang dipilih pada masa ini

## Acara tersuai

- `change`: Dicetuskan apabila tab ditukar, `e.value` ialah nilai `value` bagi teg yang sedang diaktifkan.