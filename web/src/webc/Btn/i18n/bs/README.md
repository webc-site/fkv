# Obično dugme i dugme okrugle ikone

## Ime klase

- Normalno dugme`.Btn`
- ikona dugme `.BtnC`
- Stanje primarnog isticanja`.Btn.Main`

## koristiti

Dodajte naziv klase elementu:

```html
<!-- Normalno dugme -->
<button class="Btn">potvrditi</button>

<!-- glavni naglasak -->
<button class="Btn Main">submit</button>

<!-- dugme za vezu -->
<a class="Btn" href="https://github.com">dugme za vezu</a>
```

## stil ikone

Ikona za `BtnC` se može prilagoditi putem `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```