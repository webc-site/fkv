# Sfondi i xhamit të mbuluar me brymë dhe efekti i theksimit të skajeve

Kufiri gjysmë transparent dhe hija e brendshme krijojnë thekse të skajeve me mikro-reliev.
Si varësia e stilit bazë të komponentëve të butonit dhe kutisë hyrëse.

## Përdorni demonstrimin

```html
<main class="Lg">
  <article>
    <h3>titullin</h3>
    <pre>Teksti i përmbajtjes</pre>
  </article>
</main>
```

## klasë stili

### `.Lg`
Stili bazë i kartës. Aplikoni `backdrop-filter` turbullimin, `border` kufirin me mikro-ngulitje dhe `box-shadow` hijen e brendshme. Zbato filtrin `svg/glass.svg#h` në lëvizje.

### `main.Lg`
Kontejneri kryesor i bllokut të faqes. Qoshe të rrumbullakosura `24px` me mbushje reaguese (`16px` nëse gjerësia është më e vogël se `800px`, `24px` ndryshe).

### `.Btn`, `.BtnC`, `.Input`
Emri i klasës së komponentit bazë. Pas prezantimit të këtij stili, ju do të merrni automatikisht paraqitjen fleksibël, marrëdhëniet hierarkike dhe vetitë themelore të xhamit të mbuluar me brymë.