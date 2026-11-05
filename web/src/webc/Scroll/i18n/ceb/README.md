# virtual nga scroll bar

- **Two-way scrolling**: Naghatag ug bertikal (`c-vs`) ug horizontal (`c-hs`) scrolling.
- **Gidak-on nga tubag**: Pag-monitor sa mga pagbag-o sa sulud ug gidak-on sa lugar, awtomatiko nga i-update ang gidak-on ug posisyon sa scroll bar, ug awtomatiko nga itago kung wala’y pag-awas.
- ** Interactive nga suporta**: Nagsuporta sa pag-scroll sa ligid, pag-drag sa slider ug pagpoposisyon sa pag-click sa track.
- **Status Hide**: Ipakita ug palapdan ang slider kung mag-hover o mag-scroll, ug awtomatiko kining itago kung wala’y hunong.

## bertikal nga scroll

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

## pinahigda nga scroll

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

## estilo nga mga bahin

Ipasibo ang estilo sa Shadow DOM internal nga mga elemento pinaagi sa `::part()`:

- `::part(scroll)`: Pag-scroll sa sulod nga lugar.
- `::part(bar)`: Track sa scroll bar.
- `::part(si)`: Scroll bar slider.
- `::part(i)`: Internal nga layer sa pagputos.
- `::part(drag)`: Ang scroll bar track sa dragging state.

## estilo nga klase

- `.drag`: Gipadapat sa `body` nga elemento, nga nagpakita nga kini anaa sa drag-scrolling state.

## Pasadya nga mga kabtangan

- `--si-bg`: Kolor sa background sa slider.
- `--si-anim`: Animasyon sa slider.
- `--cursorScrollhSvg`: Ang mouse cursor kung ang pinahigda nga scroll bar gipalupad.
- `--cursorScrollvSvg`: Ang mouse cursor kung ang bertikal nga scroll bar gipalupad.