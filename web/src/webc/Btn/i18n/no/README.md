# Vanlig knapp og rund ikonknapp

## Klassenavn

- Normal knapp`.Btn`
- ikonknapp `.BtnC`
- Primær fremhevingstilstand`.Btn.Main`

## bruk

Legg til et klassenavn til elementet:

```html
<!-- Normal knapp -->
<button class="Btn">bekrefte</button>

<!-- hovedhøydepunktet -->
<button class="Btn Main">sende inn</button>

<!-- link-knappen -->
<a class="Btn" href="https://github.com">link-knappen</a>
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