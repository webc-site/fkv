# Vanlig knapp och rund ikonknapp

## Klassens namn

- Normal knapp`.Btn`
- ikonknapp `.BtnC`
- Primärt markerat tillstånd`.Btn.Main`

## använda

Lägg till ett klassnamn till elementet:

```html
<!-- Normal knapp -->
<button class="Btn">bekräfta</button>

<!-- huvudhöjdpunkt -->
<button class="Btn Main">överlämna</button>

<!-- länkknapp -->
<a class="Btn" href="https://github.com">länkknapp</a>
```

## ikon stil

Ikonen för `BtnC` kan anpassas via `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```