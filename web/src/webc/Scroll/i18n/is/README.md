# sýndarskrollslá

- **Tvíhliða flun**: Veitir lóðrétta (`c-vs`) og lárétta (`c-hs`) flun.
- **Stærðarviðbrögð**: Fylgstu með breytingum á efni og svæðisstærð, uppfærðu sjálfkrafa stærð og staðsetningu skrunstikunnar og fela sjálfkrafa þegar ekkert flæði er.
- **Gagnvirkur stuðningur**: Styður skrun á hjólum, drátt sleða og staðsetningu lagsmella.
- **Staðfela**: Sýndu og breikkaðu sleðann þegar þú sveimar eða flettir, og fela hann sjálfkrafa þegar hann er kyrrstæður.

## lóðrétt fletta

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

## lárétt fletta

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

## stílhlutum

Sérsníddu stíl Shadow DOM innri þátta með `::part()`:

- `::part(scroll)`: Skrunaðu innihaldssvæðið.
- `::part(bar)`: Skrunastikur.
- `::part(si)`: Skrunastiku.
- `::part(i)`: Innra umbúðir.
- `::part(drag)`: Skrunastikan í dragstöðu.

## stílaflokkur

- `.drag`: Notað á `body` eininguna, sem gefur til kynna að það sé í drag-skrollandi ástandi.

## Sérsniðnar eignir

- `--si-bg`: Bakgrunnslitur sleða.
- `--si-anim`: Renna hreyfimynd.
- `--cursorScrollhSvg`: Músarbendillinn þegar lárétta skrunstikan er sveima.
- `--cursorScrollvSvg`: Músarbendillinn þegar lóðrétta skrunstikan er sveima.