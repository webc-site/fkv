# gesaan gelembung terapung

Paparkan maklumat petua alat tuding apabila tetikus dilegar atau elemen diaktifkan.

- Berdasarkan Komponen Web asli, ringan dan berprestasi tinggi
- Menyokong kedudukan spatial adaptif dalam empat arah (class="atas/bawah/kiri/kanan")
- Ia menggunakan latar belakang kecerunan tekstur kaca fros hitam dan aksara putih, dengan sorotan kontur tepi fizikal yang indah.
- Pengiraan penyesuaian kedudukan terbina dalam, melaraskan arah pop timbul secara automatik mengikut ruang yang tinggal dalam port pandangan
- Gunakan `slot="pop"` untuk memaparkan kandungan gesaan terapung

## Gunakan demo

```html
<!-- Kira arah secara automatik (disyorkan, tidak perlu mengisytiharkan kelas) -->
<c-pophover>
  <button>automatik</button>
  <div slot="pop">Ini adalah petua gelembung</div>
</c-pophover>

<!-- arah tetap -->
<c-pophover class="left">
  <button>kiri</button>
  <div slot="pop">Ini adalah petua gelembung</div>
</c-pophover>

<c-pophover class="right">
  <button>betul</button>
  <div slot="pop">Ini adalah petua gelembung</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Bawah</button>
  <div slot="pop">Ini adalah petua gelembung</div>
</c-pophover>

<c-pophover class="top">
  <button>atasan</button>
  <div slot="pop">Ini adalah petua gelembung</div>
</c-pophover>
```

## pembolehubah CSS

Gaya tersuai disokong melalui pembolehubah CSS berikut:

| nama pembolehubah | nilai lalai | menggambarkan |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Ruang antara petua alat gelembung dan elemen pencetus |
| `--pophover-margin` | `24px` | Margin keselamatan tepi viewport semasa pengiraan kedudukan penyesuaian |
| `--pophover-bg-top` | `#18181ce0` | Warna atas latar belakang kecerunan gelembung |
| `--pophover-bg-mid` | `#121215e5` | Warna latar belakang anak panah di sebelah kiri dan kanan gelembung |
| `--pophover-bg-bottom` | `#0c0c0feb` | Warna anak panah bahagian bawah dan atas dan bawah latar belakang kecerunan gelembung |

## Gaya petua alat ikon (`.Ico`)

Untuk butang gesaan terapung jenis ikon tulen (seperti "Edit", "Padam", dsb.), anda boleh menggunakan ikon bersatu dan gaya tuding dengan menambahkan nama kelas `.Ico` pada bekas luar atau komponen itu sendiri.

### Contoh struktur

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">edit</div>
  </c-pophover>
</b>
```

Reka letak asas, saiz (18x18px) dan perubahan warna tuding telah disatukan dalam `.Ico > c-pophover > a`. Pembangun hanya perlu menentukan fail ikon yang sepadan secara berasingan, seperti:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```