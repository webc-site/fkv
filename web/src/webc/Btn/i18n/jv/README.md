# Tombol biasa lan tombol lambang bunder

## Jeneng kelas

- Tombol normal`.Btn`
- tombol lambang `.BtnC`
- Status sorotan utama`.Btn.Main`

## nggunakake

Tambah jeneng kelas menyang unsur:

```html
<!-- Tombol normal -->
<button class="Btn">konfirmasi</button>

<!-- sorotan utama -->
<button class="Btn Main">ngirim</button>

<!-- tombol link -->
<a class="Btn" href="https://github.com">tombol link</a>
```

## gaya ikon

Ikon kanggo `BtnC` bisa disesuaikan liwat `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```