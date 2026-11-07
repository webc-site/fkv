# Arddulliau teipograffeg wedi'u hoptimeiddio ar gyfer profiad darllen

- **Cysodi sylfaenol**: Addasu i deitlau, paragraffau, print trwm, italig, llinell drwodd, cod mewnol, lluniau a llinellau rhannu.
- **Animeiddiad cyswllt**: Dangoswch animeiddiad tanlinellu wrth hofran.
- **Rhestr prosiect**: Mae rhestrau heb eu trefnu yn defnyddio bwledi, mae rhestrau archebedig yn defnyddio rhifau cyfresol cefndir crwn, ac yn cefnogi nythu aml-lefel.
- **Rhestr Tasgau**: Yn darparu arddull blwch ticio, yn cefnogi pethau i'w gwneud a statws wedi'i gwblhau.
- **Awgrymiadau Rhybuddio**: Yn cefnogi pum math o flychau rhybuddio: Nodyn, Awgrym, Pwysig, Rhybudd a Sylw.
- **Tablau a Chod**: Yn darparu aroleuo hofran rhes tabl, a blociau cod gyda chysgodion a chefndiroedd graddiant.

## Defnyddiwch y demo

```html
<div class="Md">
  <h1>teitl</h1>
  <p>Testun paragraff a <a href="#">dolen</a></p>
  <hr>
  <h2>Teitl ail lefel</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Eitem Tasg</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Nodyn y cynnwys</p>
  </blockquote>
</div>
```

## dosbarth arddull

Cymhwysir arddulliau i elfennau HTML o dan y `.Md` cynhwysydd.

### Arddull blwch rhybudd

Ychwanegwch yr enw dosbarth canlynol ar yr elfen `blockquote` gyda'r enw dosbarth `.q` i doglo'r blwch rhybuddio:

- `note`: Nodiadau
- `tip`: Awgrym
- `important`: Pwysig
- `warning`: rhybudd
- `caution`: Nodyn