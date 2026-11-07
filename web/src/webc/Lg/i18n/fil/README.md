# Frosted glass background at edge highlight effect

Ang semi-transparent na hangganan at panloob na anino ay lumilikha ng mga highlight sa gilid ng micro-relief.
Bilang pangunahing dependency ng estilo ng mga bahagi ng button at input box.

## Gamitin ang demo

```html
<main class="Lg">
  <article>
    <h3>pamagat</h3>
    <pre>Teksto ng nilalaman</pre>
  </article>
</main>
```

## klase ng istilo

### `.Lg`
Pangunahing istilo ng card. Ilapat ang `backdrop-filter` blur, `border` micro-embossed na border, at `box-shadow` inner shadow. Ilapat ang `svg/glass.svg#h` filter sa hover.

### `main.Lg`
Ang pangunahing block container ng page. Mga bilugan na sulok `24px` na may tumutugong padding (`16px` kung ang lapad ay mas mababa sa `800px`, `24px` kung hindi man).

### `.Btn`, `.BtnC`, `.Input`
Pangalan ng pangunahing bahagi ng klase. Pagkatapos ipakilala ang istilong ito, awtomatiko kang makakakuha ng flexible na layout, hierarchical na relasyon at mga pangunahing katangian ng frosted glass.