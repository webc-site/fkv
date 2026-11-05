# Bilah navigasi atas yang secara otomatis diciutkan atau diperluas saat halaman digulir

- **Sembunyikan Otomatis**: Menyembunyikan secara otomatis saat menggulir halaman ke bawah.
- **Perluas otomatis**: Ditampilkan secara otomatis saat halaman digulir ke atas atau bilah gulir menghilang.
- **Optimasi Kinerja**: Gunakan `requestAnimationFrame` untuk membatasi pengguliran.
- **Rilis sumber daya**: Pemroses dan pengamat peristiwa secara otomatis dihapus ketika komponen dimusnahkan.

## Gunakan demonya

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Link</a>
      </c-nav>
    </nav>
    <div class="content">Geser ke bawah untuk menyembunyikan navigasi, geser ke atas untuk menampilkannya</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## Petunjuk Penggunaan

Harus digunakan sebagai elemen turunan atau turunan dari `<c-vs>`.

## status gaya

Komponen mengganti nama kelas berikut pada `nav.Nav`:

- `D`: Menampilkan status (geser ke bawah atau atas).
- `I`: Status transisi disembunyikan.
- `H`: Status tersembunyi.

## variabel gaya

- `--top`: Diperbarui secara otomatis oleh komponen ke nilai ketinggian negatif, digunakan untuk penentuan posisi dan transisi.