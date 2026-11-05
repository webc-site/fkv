# virtuální posuvník

- **Obousměrné rolování**: Poskytuje vertikální (`c-vs`) a horizontální (`c-hs`) rolování.
- **Reakce na velikost**: Sledujte změny obsahu a velikosti oblasti, automaticky aktualizujte velikost a polohu posuvníku a automaticky se skryjte, když nedojde k přetečení.
- **Interaktivní podpora**: Podporuje rolování kolečkem, přetahování jezdcem a umístění klikání na stopu.
- **Skrýt stav**: Zobrazí a rozšíří posuvník při umístění kurzoru nebo posouvání a automaticky jej skryje, když stojí.

## vertikální rolování

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

## horizontální rolování

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

## stylové díly

Přizpůsobte styl vnitřních prvků Shadow DOM pomocí `::part()`:

- `::part(scroll)`: Posouvání oblasti obsahu.
- `::part(bar)`: Stopa posuvníku.
- `::part(si)`: Posuvník posuvníku.
- `::part(i)`: Vnitřní obalová vrstva.
- `::part(drag)`: Stopa posuvníku ve stavu přetažení.

## stylová třída

- `.drag`: Použito na prvek `body`, což znamená, že je ve stavu posouvání přetažením.

## Vlastní vlastnosti

- `--si-bg`: Barva pozadí posuvníku.
- `--si-anim`: Animace posuvníku.
- `--cursorScrollhSvg`: Kurzor myši při umístění vodorovného posuvníku.
- `--cursorScrollvSvg`: Kurzor myši při umístění svislého posuvníku.