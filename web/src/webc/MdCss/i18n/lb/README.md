# Typographie Stiler optimiséiert fir Lieserfarung

- ** Basis Zeechnung **: Upasst un Titelen, Abschnitter, Fett, Kursiv, duerchstrecken, Inline Code, Biller an Trennungslinnen.
- ** Link Animatioun **: Ënnersträichen Animatioun weisen wann Dir schwieft.
- ** Projet Lëscht **: Onbestallt Lëschte benotzen Bullets, bestallt Lëschte benotzen ofgerënnt Hannergrond Serien Zuelen, an ënnerstëtzen Multi-Level Nesting.
- ** Task Lëscht **: Bitt Checkbox Stil, ënnerstëtzt To-Do a fäerdeg Status.
- ** Warnungstipps **: Ënnerstëtzt fënnef Aarte vu Warnboxen: Notiz, Tipp, Wichteg, Warnung an Opmierksamkeet.
- ** Dëscher a Code **: Bitt Dësch Reih Hover Highlight, a Code Blocks mat Schatten a Gradienthannergrënn.

## Benotzt d'Demo

```html
<div class="Md">
  <h1>Titel</h1>
  <p>Paragraftext an <a href="#">Link</a></p>
  <hr>
  <h2>Zweeten Niveau Titel</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Task Item</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Notiz Inhalt</p>
  </blockquote>
</div>
```

## Stil Klass

Stiler ginn op HTML Elementer ënner dem `.Md` Container applizéiert.

### Alarm Box Stil

Füügt de folgende Klassennumm op `blockquote` Element mam `.q` Klassennumm fir d'Alarmkëscht ze wiesselen:

- `note`: Notizen
- `tip`: Tipp
- `important`: Wichteg
- `warning`: Warnung
- `caution`: Notiz