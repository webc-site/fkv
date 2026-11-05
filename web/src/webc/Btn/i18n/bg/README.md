# Обикновен бутон и бутон с кръгла икона

## Име на класа

- Нормален бутон`.Btn`
- бутон с икона `.BtnC`
- Състояние на основно осветяване`.Btn.Main`

## използване

Добавете име на клас към елемента:

```html
<!-- Нормален бутон -->
<button class="Btn">потвърди</button>

<!-- основен акцент -->
<button class="Btn Main">подавам</button>

<!-- бутон за връзка -->
<a class="Btn" href="https://github.com">бутон за връзка</a>
```

## стил на икона

Иконата за `BtnC` може да бъде персонализирана чрез `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```