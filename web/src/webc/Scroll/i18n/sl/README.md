# virtualni drsni trak

- **Dvosmerno drsenje**: Omogoča navpično (`c-vs`) in vodoravno (`c-hs`) drsenje.
- **Odziv na velikost**: Spremljajte spremembe velikosti vsebine in območja, samodejno posodobite velikost in položaj drsnega traku ter se samodejno skrijete, ko ni prelivanja.
- **Interaktivna podpora**: Podpira pomikanje s kolescem, vlečenje drsnika in pozicioniranje sledi s klikom.
- **Skrij stanje**: prikažite in razširite drsnik, ko lebdite ali se pomikate, in ga samodejno skrijete, ko miruje.

## navpično drsenje

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

## vodoravno drsenje

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

## slogovni deli

Prilagodite slog notranjih elementov Shadow DOM prek `::part()`:

- `::part(scroll)`: Pomikajte se po območju vsebine.
- `::part(bar)`: Sled drsnega traku.
- `::part(si)`: drsnik.
- `::part(i)`: notranja ovojna plast.
- `::part(drag)`: Sled drsnega traku v stanju vlečenja.

## stilski razred

- `.drag`: Uporabljeno za element `body`, ki označuje, da je v stanju vlečenja in drsenja.

## Lastnosti po meri

- `--si-bg`: Barva ozadja drsnika.
- `--si-anim`: Animacija drsnika.
- `--cursorScrollhSvg`: Kazalec miške, ko se pomakne vodoravni drsni trak.
- `--cursorScrollvSvg`: Kazalec miške, ko lebdite nad navpičnim drsnim trakom.