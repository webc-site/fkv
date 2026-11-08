# virtuelna traka za pomeranje

- **Dvosmjerno pomicanje**: Omogućuje vertikalno (`c-vs`) i horizontalno (`c-hs`) pomicanje.
- **Reakcija na veličinu**: Pratite promjene sadržaja i veličine područja, automatski ažurirajte veličinu i poziciju trake za pomicanje i automatski se sakrijte kada nema prelivanja.
- **Interaktivna podrška**: Podržava pomicanje kotačića, povlačenje klizača i pozicioniranje klika.
- **Sakrij status**: Pokažite i proširite klizač kada lebdite ili se pomičete i automatski ga sakrijte kada stoji.

## vertikalni skrol

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

## horizontalni skrol

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

Prilagodite stil Shadow DOM internih elemenata putem `::part()`:

- `::part(scroll)`: Pomičite područje sadržaja.
- `::part(bar)`: Traka za pomicanje.
- `::part(si)`: Klizač trake za pomicanje.
- `::part(i)`: Sloj unutrašnjeg omotača.
- `::part(drag)`: Traka za pomicanje u stanju prevlačenja.

## stilska klasa

- `.drag`: Primijenjeno na `body` element, što ukazuje da je u stanju povlačenja pomicanja.

## Prilagođena svojstva

- `--si-bg`: Boja pozadine klizača.
- `--si-anim`: Animacija klizača.
- `--cursorScrollhSvg`: Kursor miša kada se lebdi horizontalna traka za pomicanje.
- `--cursorScrollvSvg`: Kursor miša kada se lebdi okomita traka za pomicanje.