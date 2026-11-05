# Adi düymə və dəyirmi ikon düyməsi

## Sinif adı

- Normal düymə`.Btn`
- ikon düyməsi `.BtnC`
- Əsas vurğu vəziyyəti`.Btn.Main`

## istifadə edin

Elementə sinif adı əlavə edin:

```html
<!-- Normal düymə -->
<button class="Btn">təsdiq edin</button>

<!-- əsas vurğu -->
<button class="Btn Main">təqdim edin</button>

<!-- keçid düyməsi -->
<a class="Btn" href="https://github.com">keçid düyməsi</a>
```

## ikon stili

`BtnC` üçün ikona `background-image` vasitəsilə fərdiləşdirilə bilər:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```