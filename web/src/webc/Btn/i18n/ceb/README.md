# Ordinaryo nga buton ug lingin nga buton sa icon

## Ngalan sa klase

- Normal nga buton`.Btn`
- buton sa icon `.BtnC`
- Panguna nga highlight state`.Btn.Main`

## paggamit

Idugang ang ngalan sa klase sa elemento:

```html
<!-- Normal nga butones -->
<button class="Btn">kompirma</button>

<!-- pangunang highlight -->
<button class="Btn Main">isumiter</button>

<!-- buton sa link -->
<a class="Btn" href="https://github.com">buton sa link</a>
```

## estilo sa icon

Ang icon para sa `BtnC` mahimong ipasibo pinaagi sa `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```