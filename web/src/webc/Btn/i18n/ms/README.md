# Butang biasa dan butang ikon bulat

## Nama kelas

- Butang biasa`.Btn`
- butang ikon `.BtnC`
- Keadaan sorotan utama`.Btn.Main`

## guna

Tambahkan nama kelas pada elemen:

```html
<!-- Butang biasa -->
<button class="Btn">mengesahkan</button>

<!-- sorotan utama -->
<button class="Btn Main">menyerahkan</button>

<!-- butang pautan -->
<a class="Btn" href="https://github.com">butang pautan</a>
```

## gaya ikon

Ikon untuk `BtnC` boleh disesuaikan melalui `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```