# Gewéinleche Knäppchen a ronn Ikon Knäppchen

## Klass Numm

- Normale Knäppchen`.Btn`
- icon button `.BtnC`
- Primär Highlight Staat`.Btn.Main`

## benotzen

Füügt e Klassennumm un d'Element:

```html
<!-- Normal Knäppchen -->
<button class="Btn">confirméieren</button>

<!-- Haapt Highlight -->
<button class="Btn Main">ofginn</button>

<!-- Link Knäppchen -->
<a class="Btn" href="https://github.com">Link Knäppchen</a>
```

## Ikon Stil

D'Ikon fir `BtnC` ka personaliséiert ginn iwwer `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```