# Almindelig knap og rund ikonknap

## Klassens navn

- Normal knap`.Btn`
- ikonknap `.BtnC`
- Primær fremhævningstilstand`.Btn.Main`

## bruge

Tilføj et klassenavn til elementet:

```html
<!-- Normal knap -->
<button class="Btn">bekræfte</button>

<!-- vigtigste højdepunkt -->
<button class="Btn Main">indsende</button>

<!-- link-knap -->
<a class="Btn" href="https://github.com">link-knap</a>
```

## ikon stil

Ikonet for `BtnC` kan tilpasses via `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```