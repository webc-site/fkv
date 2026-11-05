# Estils de tipografia optimitzats per a l'experiència de lectura

- **Composició bàsica**: s'adapta als títols, paràgrafs, negreta, cursiva, ratllat, codi en línia, imatges i línies divisòries.
- **Animació d'enllaç**: mostra l'animació de subratllat en passar el cursor.
- **Llista de projectes**: les llistes no ordenades utilitzen vinyetes, les llistes ordenades utilitzen números de sèrie de fons arrodonits i admeten l'imbricació de diversos nivells.
- **Llista de tasques**: proporciona un estil de casella de selecció, admet l'estat de tasques pendents i completat.
- **Consells d'advertència**: admet cinc tipus de quadres d'advertència: Nota, Consell, Important, Advertiment i Atenció.
- **Taules i codi**: proporciona un ressaltat de fila de taula i blocs de codi amb ombres i fons degradats.

## Utilitzeu la demostració

```html
<div class="Md">
  <h1>títol</h1>
  <p>Text del paràgraf i <a href="#">enllaç</a></p>
  <hr>
  <h2>Títol de segon nivell</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Element de la tasca</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Contingut de la nota</p>
  </blockquote>
</div>
```

## classe d'estil

Els estils s'apliquen als elements HTML al contenidor `.Md`.

### Estil de caixa d'alerta

Afegiu el nom de classe següent a l'element `blockquote` amb el nom de classe `.q` per canviar el quadre d'alerta:

- `note`: notes
- `tip`: Consell
- `important`: important
- `warning`: advertència
- `caution`: Nota