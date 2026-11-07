# virtual na scroll bar

- **Two-way scrolling**: Nagbibigay ng vertical (`c-vs`) at horizontal (`c-hs`) scrolling.
- **Size response**: Subaybayan ang content at mga pagbabago sa laki ng lugar, awtomatikong i-update ang laki at posisyon ng scroll bar, at awtomatikong itago kapag walang overflow.
- **Interactive na suporta**: Sinusuportahan ang pag-scroll ng gulong, pag-drag ng slider at pagpoposisyon ng pag-click sa track.
- **Status Hide**: Ipakita at palawakin ang slider kapag nagho-hover o nag-i-scroll, at awtomatikong itago ito kapag nakatigil.

## patayong scroll

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

## pahalang na scroll

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

## mga bahagi ng istilo

I-customize ang istilo ng mga panloob na elemento ng Shadow DOM sa pamamagitan ng `::part()`:

- `::part(scroll)`: Mag-scroll sa lugar ng nilalaman.
- `::part(bar)`: Track ng scroll bar.
- `::part(si)`: Scroll bar slider.
- `::part(i)`: Panloob na pambalot na layer.
- `::part(drag)`: Ang track ng scroll bar sa estado ng pag-drag.

## klase ng istilo

- `.drag`: Inilapat sa elemento ng `body`, na nagsasaad na ito ay nasa estado ng pag-drag-scroll.

## Mga custom na property

- `--si-bg`: Kulay ng background ng slider.
- `--si-anim`: Slider animation.
- `--cursorScrollhSvg`: Ang cursor ng mouse kapag naka-hover ang pahalang na scroll bar.
- `--cursorScrollvSvg`: Ang cursor ng mouse kapag naka-hover ang vertical scroll bar.