# Butoni i zakonshëm dhe butoni i ikonës së rrumbullakët

## Emri i klasës

- Butoni normal`.Btn`
- butoni i ikonës `.BtnC`
- Gjendja kryesore e theksimit`.Btn.Main`

## përdorni

Shtoni një emër klase në element:

```html
<!-- Butoni normal -->
<button class="Btn">konfirmoj</button>

<!-- pika kryesore -->
<button class="Btn Main">dorëzojë</button>

<!-- butoni i lidhjes -->
<a class="Btn" href="https://github.com">butoni i lidhjes</a>
```

## stil ikonë

Ikona për `BtnC` mund të personalizohet nëpërmjet `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```