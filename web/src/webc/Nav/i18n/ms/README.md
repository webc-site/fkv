# Bar navigasi teratas yang runtuh atau berkembang secara automatik apabila halaman menatal

- **Auto Sembunyi**: Sembunyikan secara automatik apabila menatal ke bawah halaman.
- **Auto kembangkan**: Dipaparkan secara automatik apabila halaman ditatal ke atas atau bar tatal hilang.
- **Pengoptimuman Prestasi**: Gunakan `requestAnimationFrame` untuk mendikit penatalan.
- **Sumber keluaran**: Pendengar dan pemerhati acara dialih keluar secara automatik apabila komponen dimusnahkan.

## Gunakan demo

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Pautan</a>
      </c-nav>
    </nav>
    <div class="content">Leret ke bawah untuk menyembunyikan navigasi, leret ke atas untuk menunjukkannya</div>
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

## Arahan penggunaan

Mesti digunakan sebagai anak atau unsur keturunan `<c-vs>`.

## status gaya

Komponen menukar nama kelas berikut pada `nav.Nav`:

- `D`: Tunjukkan status (slaid ke bawah atau di atas).
- `I`: Keadaan peralihan sedang disembunyikan.
- `H`: Status tersembunyi.

## pembolehubah gaya

- `--top`: Kemas kini secara automatik oleh komponen kepada nilai negatif ketinggian, digunakan untuk kedudukan dan peralihan.