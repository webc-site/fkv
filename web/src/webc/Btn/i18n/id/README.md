# Tombol biasa dan tombol ikon bulat

## Nama kelas

- Tombol biasa`.Btn`
- tombol ikon `.BtnC`
- Status sorotan utama`.Btn.Main`

## menggunakan

Tambahkan nama kelas ke elemen:

```html
<!-- Tombol biasa -->
<button class="Btn">mengonfirmasi</button>

<!-- sorotan utama -->
<button class="Btn Main">kirim</button>

<!-- tombol tautan -->
<a class="Btn" href="https://github.com">tombol tautan</a>
```

## gaya ikon

Ikon untuk `BtnC` dapat dikustomisasi melalui `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```