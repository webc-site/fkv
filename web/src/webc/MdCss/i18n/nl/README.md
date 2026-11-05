# Typografiestijlen geoptimaliseerd voor leeservaring

- **Basis typografie**: Aanpassen aan titels, alinea's, vetgedrukt, cursief, doorhalen, inline code, afbeeldingen en scheidingslijnen.
- **Linkanimatie**: Onderstrepingsanimatie weergeven tijdens het zweven.
- **Projectlijst**: ongeordende lijsten gebruiken opsommingstekens, geordende lijsten gebruiken afgeronde serienummers op de achtergrond en ondersteunen nesten op meerdere niveaus.
- **Takenlijst**: biedt de stijl van selectievakjes, ondersteunt taken en voltooide status.
- **Waarschuwingstips**: Ondersteunt vijf soorten waarschuwingsvensters: Opmerking, Tip, Belangrijk, Waarschuwing en Attentie.
- **Tabellen en code**: Biedt markering van zwevende tabelrijen en codeblokken met schaduwen en verloopachtergronden.

## Gebruik de demo

```html
<div class="Md">
  <h1>titel</h1>
  <p>Alineatekst en <a href="#">link</a></p>
  <hr>
  <h2>Titel van het tweede niveau</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Taakitem</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Notitie inhoud</p>
  </blockquote>
</div>
```

## stijl klasse

Stijlen worden toegepast op HTML-elementen onder de `.Md` container.

### Stijl van waarschuwingsbox

Voeg de volgende klassenaam toe aan het `blockquote` element met de `.q` klassenaam om het waarschuwingsvenster in of uit te schakelen:

- `note`: notities
- `tip`: Tip
- `important`: belangrijk
- `warning`: waarschuwing
- `caution`: Opmerking