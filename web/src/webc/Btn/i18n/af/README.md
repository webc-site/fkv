# Gewone knoppie en ronde ikoonknoppie

## Klas naam

- Normale knoppie`.Btn`
- ikoonknoppie `.BtnC`
- Primêre hoogtepunt toestand`.Btn.Main`

## gebruik

Voeg 'n klasnaam by die element:

```html
<!-- Normale knoppie -->
<button class="Btn">bevestig</button>

<!-- hoof hoogtepunt -->
<button class="Btn Main">indien</button>

<!-- skakel knoppie -->
<a class="Btn" href="https://github.com">skakel knoppie</a>
```

## ikoon styl

Die ikoon vir `BtnC` kan aangepas word via `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```