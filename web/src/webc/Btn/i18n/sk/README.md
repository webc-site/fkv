# Obyčajné tlačidlo a tlačidlo s okrúhlou ikonou

## Názov triedy

- Normálne tlačidlo`.Btn`
- tlačidlo ikony `.BtnC`
- Primárny stav zvýraznenia`.Btn.Main`

## použitie

Pridajte k prvku názov triedy:

```html
<!-- Normálne tlačidlo -->
<button class="Btn">potvrdiť</button>

<!-- hlavný vrchol -->
<button class="Btn Main">predložiť</button>

<!-- tlačidlo odkazu -->
<a class="Btn" href="https://github.com">tlačidlo odkazu</a>
```

## štýl ikony

Ikonu pre `BtnC` je možné prispôsobiť pomocou `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```