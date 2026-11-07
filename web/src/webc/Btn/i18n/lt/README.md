# Įprastas mygtukas ir apvalios piktogramos mygtukas

## Klasės pavadinimas

- Įprastas mygtukas`.Btn`
- piktogramos mygtukas `.BtnC`
- Pirminio paryškinimo būsena`.Btn.Main`

## naudoti

Pridėkite klasės pavadinimą prie elemento:

```html
<!-- Įprastas mygtukas -->
<button class="Btn">patvirtinti</button>

<!-- pagrindinis akcentas -->
<button class="Btn Main">pateikti</button>

<!-- nuorodos mygtuką -->
<a class="Btn" href="https://github.com">nuorodos mygtuką</a>
```

## piktogramos stilius

`BtnC` piktogramą galima tinkinti naudojant `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```