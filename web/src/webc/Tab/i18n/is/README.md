# Skiptanlegur flipar

- Valið ástand merkisins er óaðfinnanlega samþætt innihaldssvæðinu fyrir neðan.
- Byggingarforskriftir: Undireiningarnar eru fastar til að nota `nav > a` sem merkimiðaleiðsögn og beini undirhlutinn `b` er notaður sem innihaldssvæði.
- Tengdu `a[value]` merkið sjálfkrafa við `b[slot]` innihaldshnútinn.
- Styður sjálfkrafa viðvarandi núverandi virku síðu í `localStorage` í gegnum `key` eigindina.
- Styður að tilgreina virkjaða flipasíðuna í gegnum `value` eigindina (ef það er ekkert skyndiminni og `value` verður merkið með `class="A"` eða fyrsta merkið sjálfgefið valið).
- Létt létt DOM útfærsla, engin Shadow DOM hindrun, mikið frelsi í sérsniðnum stíl.

## Notaðu kynninguna

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">epli</a>
    <a value="banana">banani</a>
    <a value="orange">mandarínu</a>
  </nav>
  <b slot="apple">Epli er næringarríkur ávöxtur</b>
  <b slot="banana">Banani er suðrænn ávöxtur</b>
  <b slot="orange">Appelsínur eru ríkar af C-vítamíni</b>
</c-tab>
```

## eign

- `key`: lykilheiti notað fyrir staðbundið geymsluþol
- `value`: valið gildi

## Sérsniðnir viðburðir

- `change`: Kveikt þegar skipt er um flipann, `e.value` er `value` gildi merkis sem nú er virkjað.