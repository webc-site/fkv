# Botó normal i botó d'icona rodona

## Nom de la classe

- Botó normal`.Btn`
- botó d'icona `.BtnC`
- Estat destacat principal`.Btn.Main`

## utilitzar

Afegiu un nom de classe a l'element:

```html
<!-- Botó normal -->
<button class="Btn">confirmar</button>

<!-- destacat principal -->
<button class="Btn Main">presentar</button>

<!-- botó d'enllaç -->
<a class="Btn" href="https://github.com">botó d'enllaç</a>
```

## estil d'icona

La icona de `BtnC` es pot personalitzar mitjançant `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```