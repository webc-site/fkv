# Obyčejné tlačítko a tlačítko s kulatou ikonou

## Název třídy

- Normální tlačítko`.Btn`
- tlačítko ikony `.BtnC`
- Primární stav zvýraznění`.Btn.Main`

## použití

Přidejte k prvku název třídy:

```html
<!-- Normální tlačítko -->
<button class="Btn">potvrdit</button>

<!-- hlavní zvýraznění -->
<button class="Btn Main">předložit</button>

<!-- tlačítko odkazu -->
<a class="Btn" href="https://github.com">tlačítko odkazu</a>
```

## styl ikony

Ikonu pro `BtnC` lze přizpůsobit pomocí `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```