# Typografistiler optimalisert for leseopplevelse

- **Grunnleggende skriftsetting**: Tilpass til titler, avsnitt, fet skrift, kursiv, gjennomstreking, innebygd kode, bilder og skillelinjer.
- **Koblingsanimasjon**: Vis understrekingsanimasjon når du svever.
- **Prosjektliste**: Uordnede lister bruker kuler, ordnede lister bruker avrundede bakgrunnsserienumre og støtter nesting på flere nivåer.
- **Oppgaveliste**: Gir avkrysningsboksstil, støtter gjøremål og fullført status.
- **Advarselstips**: Støtter fem typer advarselsbokser: Merknad, Tips, Viktig, Advarsel og Oppmerksomhet.
- **Tabeller og kode**: Gir utheving av tabellrader og kodeblokker med skygger og gradientbakgrunner.

## Bruk demoen

```html
<div class="Md">
  <h1>tittel</h1>
  <p>Avsnittstekst og <a href="#">kobling</a></p>
  <hr>
  <h2>Tittel på andre nivå</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Oppgaveelement</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Notatinnhold</p>
  </blockquote>
</div>
```

## stilklasse

Stiler brukes på HTML-elementer under `.Md`-beholderen.

### Varslingsboksstil

Legg til følgende klassenavn på `blockquote`-elementet med `.q` klassenavnet for å veksle mellom varslingsboksen:

- `note`: Merknader
- `tip`: Tips
- `important`: Viktig
- `warning`: advarsel
- `caution`: Merk