# Leturgerð stíll fínstilltur fyrir lestrarupplifun

- **Grunnsnið**: Aðlaga að titlum, málsgreinum, feitletrun, skáletri, yfirstrikun, innbyggðum kóða, myndum og deililínum.
- **Tengill hreyfimynd**: Birta undirstrikað hreyfimynd þegar þú sveimar.
- **Verkefnalisti**: Óraðaðir listar nota byssukúlur, raðaðir listar nota ávöl raðnúmer í bakgrunni og styðja hreiður á mörgum stigum.
- **Verkefnalisti**: Býður upp á gátreitastíl, styður verkefni og stöðu lokið.
- **Viðvörunarráð**: Styður fimm gerðir viðvörunarkassa: Athugið, Ábending, Mikilvægt, Viðvörun og Athugið.
- **Töflur og kóða**: Veitir auðkenningu á töfluröðum og kóðablokkum með skugga og hallabakgrunni.

## Notaðu kynninguna

```html
<div class="Md">
  <h1>titill</h1>
  <p>Málsgreinartexti og <a href="#">tengill</a></p>
  <hr>
  <h2>Titill á öðru stigi</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Verkefni</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Athugasemd</p>
  </blockquote>
</div>
```

## stílaflokkur

Stílar eru notaðir á HTML þætti undir `.Md` ílátinu.

### Stíll viðvörunarkassa

Bættu eftirfarandi flokksheiti við `blockquote` frumefninu með `.q` flokksheitinu til að skipta um viðvörunarboxið:

- `note`: Athugasemdir
- `tip`: Ábending
- `important`: Mikilvægt
- `warning`: viðvörun
- `caution`: Athugið