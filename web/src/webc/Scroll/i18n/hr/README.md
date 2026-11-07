# virtualna traka za pomicanje

- **Dvosmjerno pomicanje**: Omogućuje okomito (`c-vs`) i vodoravno (`c-hs`) pomicanje.
- **Odgovor na veličinu**: Prati promjene sadržaja i područja, automatski ažurira veličinu i položaj klizača, i automatski skriva kada nema preljeva.
- **Interaktivna podrška**: Podržava pomicanje kotačićem, povlačenje klizača i pozicioniranje traga klikom.
- **Sakrij status**: Prikaži i proširi klizač kada lebdiš ili se pomičeš i automatski ga sakrij kada miruje.

## okomito pomicanje

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## horizontalno pomicanje

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## stilski dijelovi

Prilagodite stil unutarnjih elemenata Shadow DOM putem `::part()`:

- `::part(scroll)`: Pomičite područje sadržaja.
- `::part(bar)`: Traka klizača.
- `::part(si)`: klizač trake za pomicanje.
- `::part(i)`: unutarnji sloj za omatanje.
- `::part(drag)`: Staza trake za pomicanje u stanju povlačenja.

## stilska klasa

- `.drag`: Primijenjeno na element `body`, označavajući da je u stanju povlačenja i pomicanja.

## Prilagođena svojstva

- `--si-bg`: Boja pozadine klizača.
- `--si-anim`: Animacija klizača.
- `--cursorScrollhSvg`: Pokazivač miša kada se pomakne vodoravna traka za pomicanje.
- `--cursorScrollvSvg`: kursor miša kada lebdite okomitom trakom za pomicanje.