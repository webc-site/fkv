# Oddiy tugma va dumaloq belgi tugmasi

## Sinf nomi

- Oddiy tugma`.Btn`
- belgisi tugmasi `.BtnC`
- Asosiy ajratib ko'rsatish holati`.Btn.Main`

## foydalanish

Elementga sinf nomini qo'shing:

```html
<!-- Oddiy tugma -->
<button class="Btn">tasdiqlang</button>

<!-- asosiy ta'kidlash -->
<button class="Btn Main">topshirish</button>

<!-- havola tugmasi -->
<a class="Btn" href="https://github.com">havola tugmasi</a>
```

## ikonka uslubi

`BtnC` belgisini `background-image` orqali sozlash mumkin:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```