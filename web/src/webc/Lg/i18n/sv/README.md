# Frostat glasbakgrund och kantmarkeringseffekt

Halvtransparent kant och inre skugga skapar mikrorelief kanthöjdpunkter.
Som det grundläggande stilberoendet för knapp- och inmatningsboxkomponenter.

## Använd demon

```html
<main class="Lg">
  <article>
    <h3>titel</h3>
    <pre>Innehållstext</pre>
  </article>
</main>
```

## stilklass

### `.Lg`
Kort grundläggande stil. Applicera `backdrop-filter` oskärpa, `border` mikropräglad kant och `box-shadow` inre skugga. Använd `svg/glass.svg#h` filter när du håller muspekaren.

### `main.Lg`
Sidans huvudblockbehållare. Rundade hörn `24px` med responsiv stoppning (`16px` om bredden är mindre än `800px`, `24px` annars).

### `.Btn`, `.BtnC`, `.Input`
Klassnamn för grundläggande komponent. Efter att ha introducerat denna stil kommer du automatiskt att få flexibel layout, hierarkiska relationer och grundläggande egenskaper hos frostat glas.