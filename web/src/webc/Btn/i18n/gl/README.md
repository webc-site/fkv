# Botón ordinario e botón de icona redonda

## Nome da clase

- Botón normal`.Btn`
- botón de icona `.BtnC`
- Estado de resaltado principal`.Btn.Main`

## usar

Engade un nome de clase ao elemento:

```html
<!-- Botón normal -->
<button class="Btn">confirmar</button>

<!-- destacado principal -->
<button class="Btn Main">enviar</button>

<!-- botón de ligazón -->
<a class="Btn" href="https://github.com">botón de ligazón</a>
```

## estilo de icona

A icona de `BtnC` pódese personalizar mediante `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```