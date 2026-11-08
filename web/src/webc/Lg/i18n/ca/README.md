# Fons de vidre esmerilat i efecte de ressaltat de vora

La vora semitransparent i l'ombra interior creen punts destacats de la vora en micro-relleu.
Com a dependència d'estil bàsica dels components del botó i del quadre d'entrada.

## Utilitzeu la demostració

```html
<main class="Lg">
  <article>
    <h3>títol</h3>
    <pre>Text de contingut</pre>
  </article>
</main>
```

## classe d'estil

### `.Lg`
Estil bàsic de la targeta. Apliqueu `backdrop-filter` desenfocament, `border` voreja micro-relleu i `box-shadow` ombra interior. Aplica el filtre `svg/glass.svg#h` al passar el cursor.

### `main.Lg`
El contenidor de blocs principal de la pàgina. Cantons arrodonits `24px` amb farciment sensible (`16px` si l'amplada és inferior a `800px`, `24px` en cas contrari).

### `.Btn`, `.BtnC`, `.Input`
Nom de classe de component bàsic. Després d'introduir aquest estil, obtindreu automàticament un disseny flexible, relacions jeràrquiques i propietats bàsiques del vidre esmerilat.