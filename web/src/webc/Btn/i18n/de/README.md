# Gewöhnlicher Knopf und runder Symbolknopf

## Klassenname

- Normale Schaltfläche`.Btn`
- Symbolschaltfläche `.BtnC`
- Primärer Hervorhebungsstatus`.Btn.Main`

## verwenden

Fügen Sie dem Element einen Klassennamen hinzu:

```html
<!-- Normale Taste -->
<button class="Btn">bestätigen</button>

<!-- Haupthighlight -->
<button class="Btn Main">einreichen</button>

<!-- Link-Button -->
<a class="Btn" href="https://github.com">Link-Button</a>
```

## Symbolstil

Das Symbol für `BtnC` kann über `background-image` angepasst werden:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```