# Tavaline nupp ja ümmargune ikooninupp

## Klassi nimi

- Tavaline nupp`.Btn`
- ikooni nupp `.BtnC`
- Esmane esiletõstmise olek`.Btn.Main`

## kasutada

Lisage elemendile klassi nimi:

```html
<!-- Tavaline nupp -->
<button class="Btn">kinnitada</button>

<!-- peamine esiletõst -->
<button class="Btn Main">esitama</button>

<!-- lingi nupp -->
<a class="Btn" href="https://github.com">lingi nupp</a>
```

## ikooni stiil

Ikooni `BtnC` saab kohandada `background-image` kaudu:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```