# Frosted glass nga background ug edge highlight effect

Ang semi-transparent nga border ug inner shadow nagmugna og micro-relief edge highlights.
Ingon nga sukaranan nga dependency sa istilo sa buton ug mga sangkap sa kahon sa input.

## Gamita ang demo

```html
<main class="Lg">
  <article>
    <h3>titulo</h3>
    <pre>Teksto sa sulod</pre>
  </article>
</main>
```

## estilo nga klase

### `.Lg`
Batakang estilo sa kard. Ibutang ang `backdrop-filter` blur, `border` micro-embossed border, ug `box-shadow` inner shadow. Ibutang ang `svg/glass.svg#h` filter sa hover.

### `main.Lg`
Ang nag-unang bloke nga sudlanan sa panid. Mga lingin nga eskina `24px` nga adunay responsive nga padding (`16px` kon ang gilapdon ubos sa `800px`, `24px` kon dili).

### `.Btn`, `.BtnC`, `.Input`
Panguna nga ngalan sa klase sa sangkap. Pagkahuman sa pagpaila niini nga istilo, awtomatiko ka nga makakuha og flexible nga layout, hierarchical nga relasyon ug sukaranan nga mga kabtangan sa nagyelo nga baso.