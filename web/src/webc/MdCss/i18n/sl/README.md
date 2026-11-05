# Slogi tipografije, optimizirani za bralno izkušnjo

- **Osnovno stavljanje**: Prilagodi se naslovom, odstavkom, krepkemu tisku, poševnemu tisku, prečrtanemu, vstavljeni kodi, slikam in ločnicam.
- **Animacija povezave**: Prikažite podčrtano animacijo, ko lebdite.
- **Seznam projektov**: Neurejeni seznami uporabljajo oznake, urejeni seznami uporabljajo zaokrožene serijske številke v ozadju in podpirajo gnezdenje na več ravneh.
- **Seznam opravil**: Zagotavlja slog potrditvenega polja, podpira stanje opravil in dokončano.
- **Opozorilni nasveti**: podpira pet vrst opozorilnih polj: Opomba, Nasvet, Pomembno, Opozorilo in Pozor.
- **Tabele in koda**: Omogoča označevanje vrstice tabele z miško in bloke kode s sencami in gradientnimi ozadji.

## Uporabite demo

```html
<div class="Md">
  <h1>naslov</h1>
  <p>Besedilo odstavka in <a href="#">povezava</a></p>
  <hr>
  <h2>Naslov druge stopnje</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Element opravila</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Vsebina zapiska</p>
  </blockquote>
</div>
```

## stilski razred

Slogi so uporabljeni za elemente HTML v vsebniku `.Md`.

### Slog opozorilnega polja

Dodajte naslednje ime razreda elementu `blockquote` z imenom razreda `.q`, da preklopite polje z opozorilom:

- `note`: Opombe
- `tip`: Nasvet
- `important`: Pomembno
- `warning`: opozorilo
- `caution`: Opomba