# Obični gumb i gumb s okruglom ikonom

## Naziv klase

- Normalni gumb`.Btn`
- ikona gumb `.BtnC`
- Stanje primarnog isticanja`.Btn.Main`

## koristiti

Dodajte naziv klase elementu:

```html
<!-- Normalno dugme -->
<button class="Btn">potvrditi</button>

<!-- glavni vrhunac -->
<button class="Btn Main">podnijeti</button>

<!-- gumb poveznice -->
<a class="Btn" href="https://github.com">gumb poveznice</a>
```

## stil ikone

Ikona za `BtnC` može se prilagoditi putem `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```