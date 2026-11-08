# Tipografski stilovi optimizirani za iskustvo čitanja

- **Osnovno kucanje teksta**: Prilagodite naslove, pasuse, podebljano, kurziv, precrtano, inline kod, slike i razdjelne linije.
- **Animacija veze**: Prikažite podvučenu animaciju kada lebdite.
- **Lista projekata**: Neuređene liste koriste oznake za nabrajanje, uređene liste koriste zaokružene pozadinske serijske brojeve i podržavaju ugniježđenje na više nivoa.
- **Lista zadataka**: Pruža stil okvira za potvrdu, podržava status obaveza i završen.
- **Savjeti upozorenja**: Podržava pet tipova okvira upozorenja: Napomena, Savjet, Važno, Upozorenje i Pažnja.
- **Tabele i kod**: Pruža isticanje lebdećih redova tabele i blokove koda sa senkama i pozadinom preliva.

## Koristite demo

```html
<div class="Md">
  <h1>naslov</h1>
  <p>Tekst pasusa i <a href="#">link</a></p>
  <hr>
  <h2>Titula drugog nivoa</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Stavka zadatka</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Sadržaj bilješke</p>
  </blockquote>
</div>
```

## stilska klasa

Stilovi se primjenjuju na HTML elemente ispod `.Md` kontejnera.

### Stil kutije za upozorenje

Dodajte sljedeće ime klase na element `blockquote` sa imenom klase `.q` da biste uključili okvir upozorenja:

- `note`: Bilješke
- `tip`: Savjet
- `important`: Važno
- `warning`: upozorenje
- `caution`: Napomena