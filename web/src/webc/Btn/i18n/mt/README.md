# Buttuna ordinarja u buttuna tonda tal-ikona

## Isem tal-klassi

- Buttuna normali`.Btn`
- buttuna tal-ikona `.BtnC`
- Stat ta' enfasi primarja`.Btn.Main`

## użu

Żid isem tal-klassi mal-element:

```html
<!-- Buttuna normali -->
<button class="Btn">tikkonferma</button>

<!-- jenfasizzaw prinċipali -->
<button class="Btn Main">jissottometti</button>

<!-- buttuna tal-link -->
<a class="Btn" href="https://github.com">buttuna tal-link</a>
```

## stil tal-ikona

L-ikona għal `BtnC` tista' tiġi personalizzata permezz ta' `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```