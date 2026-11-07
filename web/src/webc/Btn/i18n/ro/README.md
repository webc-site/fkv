# Buton obișnuit și butonul pictogramă rotund

## Numele clasei

- Buton normal`.Btn`
- butonul pictogramă `.BtnC`
- Stare de evidențiere principală`.Btn.Main`

## utilizare

Adăugați un nume de clasă la element:

```html
<!-- Buton normal -->
<button class="Btn">confirma</button>

<!-- principalul punct culminant -->
<button class="Btn Main">depune</button>

<!-- butonul link -->
<a class="Btn" href="https://github.com">butonul link</a>
```

## stilul pictogramei

Pictograma pentru `BtnC` poate fi personalizată prin `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```