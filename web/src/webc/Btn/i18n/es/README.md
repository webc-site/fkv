# Botón ordinario y botón de icono redondo.

## nombre de clase

- Botón normal`.Btn`
- botón icono `.BtnC`
- Estado resaltado principal`.Btn.Main`

## usar

Agregue un nombre de clase al elemento:

```html
<!-- botón normal -->
<button class="Btn">confirmar</button>

<!-- punto culminante principal -->
<button class="Btn Main">entregar</button>

<!-- botón de enlace -->
<a class="Btn" href="https://github.com">botón de enlace</a>
```

## estilo de icono

El icono de `BtnC` se puede personalizar mediante `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```