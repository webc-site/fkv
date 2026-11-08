# gelembung mengambang cepat

Menampilkan informasi tooltip hover saat mouse diarahkan atau elemen diaktifkan.

- Berdasarkan Komponen Web asli, ringan dan berkinerja tinggi
- Mendukung pemosisian spasial adaptif dalam empat arah (class="top/bottom/left/right")
- Ini menggunakan latar belakang gradien tekstur kaca buram hitam dan karakter putih, dengan highlight kontur tepi fisik yang indah.
- Perhitungan adaptif posisi bawaan, secara otomatis menyesuaikan arah pop-up sesuai dengan ruang yang tersisa di area pandang
- Gunakan `slot="pop"` untuk menampilkan konten prompt mengambang

## Gunakan demonya

```html
<!-- Menghitung arah secara otomatis (disarankan, tidak perlu mendeklarasikan kelas) -->
<c-pophover>
  <button>otomatis</button>
  <div slot="pop">Ini adalah tip gelembung</div>
</c-pophover>

<!-- arah tetap -->
<c-pophover class="left">
  <button>Kiri</button>
  <div slot="pop">Ini adalah tip gelembung</div>
</c-pophover>

<c-pophover class="right">
  <button>Kanan</button>
  <div slot="pop">Ini adalah tip gelembung</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Turun</button>
  <div slot="pop">Ini adalah tip gelembung</div>
</c-pophover>

<c-pophover class="top">
  <button>unggul</button>
  <div slot="pop">Ini adalah tip gelembung</div>
</c-pophover>
```

## Variabel CSS

Gaya khusus didukung melalui variabel CSS berikut:

| nama variabel | nilai bawaan | menjelaskan |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Ruang antara tooltip gelembung dan elemen pemicu |
| `--pophover-margin` | `24px` | Margin keamanan tepi area pandang selama penghitungan posisi adaptif |
| `--pophover-bg-top` | `#18181ce0` | Warna atas latar belakang gradien gelembung |
| `--pophover-bg-mid` | `#121215e5` | Warna latar belakang panah di sisi kiri dan kanan gelembung |
| `--pophover-bg-bottom` | `#0c0c0feb` | Warna panah sisi bawah dan atas serta bawah latar belakang gradien gelembung |

## Gaya keterangan alat ikon (`.Ico`)

Untuk tombol prompt mengambang tipe ikon murni (seperti "Edit", "Hapus", dll.), Anda dapat menerapkan ikon terpadu dan gaya hover dengan menambahkan nama kelas `.Ico` ke wadah luar atau komponen itu sendiri.

### Contoh struktur

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">sunting</div>
  </c-pophover>
</b>
```

Perubahan tata letak dasar, ukuran (18x18px), dan warna hover telah disatukan di `.Ico > c-pophover > a`. Pengembang hanya perlu menentukan file ikon yang sesuai secara terpisah, seperti:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```