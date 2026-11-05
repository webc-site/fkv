# Typografistile optimeret til læseoplevelse

- **Grundlæggende sætning**: Tilpas til titler, afsnit, fed skrift, kursiv, gennemstregning, indlejret kode, billeder og skillelinjer.
- **Link-animation**: Vis understregningsanimation, når du svæver.
- **Projektliste**: Uordnede lister bruger punkttegn, ordnede lister bruger afrundede baggrundsserienumre og understøtter indlejring på flere niveauer.
- **Opgaveliste**: Giver afkrydsningsfeltstil, understøtter gøremål og fuldført status.
- **Advarselstip**: Understøtter fem typer advarselsbokse: Bemærk, Tip, Vigtigt, Advarsel og Opmærksomhed.
- **Tabeller og kode**: Giver fremhævelse af tabelrækker og kodeblokke med skygger og gradientbaggrunde.

## Brug demoen

```html
<div class="Md">
  <h1>titel</h1>
  <p>Afsnitstekst og <a href="#">link</a></p>
  <hr>
  <h2>Titel på andet niveau</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Opgavepunkt</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Noteindhold</p>
  </blockquote>
</div>
```

## stil klasse

Typografier anvendes på HTML-elementer under `.Md` containeren.

### Alarmboks-stil

Tilføj følgende klassenavn på `blockquote` elementet med `.q` klassenavn for at skifte advarselsboksen:

- `note`: Bemærkninger
- `tip`: Tip
- `important`: Vigtigt
- `warning`: advarsel
- `caution`: Bemærk