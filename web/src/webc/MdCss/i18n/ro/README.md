# Stiluri de tipografie optimizate pentru experiența de lectură

- **Compunere de bază**: adaptați la titluri, paragrafe, aldine, cursive, barat, cod inline, imagini și linii de divizare.
- ** Animație link**: afișați animația subliniată când treceți cu mouse-ul.
- **Lista de proiecte**: listele neordonate folosesc marcatori, listele ordonate folosesc numere de serie rotunjite de fundal și acceptă imbricarea pe mai multe niveluri.
- **Lista de activități**: oferă stilul casetei de selectare, acceptă starea de rezolvat și finalizat.
- **Sfaturi de avertizare**: acceptă cinci tipuri de casete de avertizare: Notă, Sfat, Important, Avertisment și Atenție.
- **Tabele și cod**: oferă evidențierea rândurilor de tabel și blocuri de cod cu umbre și fundaluri gradiente.

## Utilizați demonstrația

```html
<div class="Md">
  <h1>titlu</h1>
  <p>Textul paragrafului și <a href="#">link</a></p>
  <hr>
  <h2>Titlu de nivel al doilea</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Element de sarcină</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Notă conținutul</p>
  </blockquote>
</div>
```

## clasa de stil

Stilurile sunt aplicate elementelor HTML din containerul `.Md`.

### Stilul casetei de alertă

Adăugați următorul nume de clasă pe elementul `blockquote` cu numele de clasă `.q` pentru a comuta caseta de alertă:

- `note`: Note
- `tip`: Sfat
- `important`: Important
- `warning`: avertisment
- `caution`: Notă