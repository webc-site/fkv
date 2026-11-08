# Botão comum e botão de ícone redondo

## Nome da classe

- Botão normal`.Btn`
- botão de ícone `.BtnC`
- Estado de destaque primário`.Btn.Main`

## usar

Adicione um nome de classe ao elemento:

```html
<!-- Botão normal -->
<button class="Btn">confirmar</button>

<!-- destaque principal -->
<button class="Btn Main">enviar</button>

<!-- botão de link -->
<a class="Btn" href="https://github.com">botão de link</a>
```

## estilo de ícone

O ícone para `BtnC` pode ser personalizado via `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```