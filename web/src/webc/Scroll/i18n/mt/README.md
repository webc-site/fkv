# iscroll bar virtwali

- **Scrolling f'żewġ direzzjonijiet**: Jipprovdi scrolling vertikali (`c-vs`) u orizzontali (`c-hs`).
- **Reazzjoni tad-daqs**: Tissorvelja l-bidliet fid-daqs tal-kontenut u taż-żona, taġġorna awtomatikament id-daqs u l-pożizzjoni tal-iscroll bar, u awtomatikament aħbi meta ma jkunx hemm overflow.
- **Appoġġ interattiv**: Jappoġġja l-iskrolljar tar-roti, it-tkaxkir tas-slajder u l-ippożizzjonar tal-ikklikkjar tal-binarji.
- **Status Hide**: Uri u twessa' s-slider meta tiċċaqlaq jew tiskrollja, u aħbiha awtomatikament meta tkun wieqfa.

## iskrollja vertikali

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

## scroll orizzontali

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

## partijiet stil

Ippersonalizza l-istil tal-elementi interni Shadow DOM permezz ta' `::part()`:

- `::part(scroll)`: Skrollja ż-żona tal-kontenut.
- `::part(bar)`: Scroll bar track.
- `::part(si)`: Scroll bar slider.
- `::part(i)`: Saff intern tat-tgeżwir.
- `::part(drag)`: Il-binarju tal-iscroll bar fl-istat tat-tkaxkir.

## klassi tal-istil

- `.drag`: Applikat għall-element `body`, li jindika li jinsab fi stat ta' drag-scrolling.

## Proprjetajiet tad-dwana

- `--si-bg`: Kulur fl-isfond tas-slajder.
- `--si-anim`: Animazzjoni slider.
- `--cursorScrollhSvg`: Il-cursor tal-maws meta l-bar tal-iskrolljar orizzontali titmexxa 'l fuq.
- `--cursorScrollvSvg`: Il-cursor tal-maws meta l-iscroll bar vertikali titmexxa 'l fuq.