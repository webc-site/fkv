# Tipogrāfijas stili, kas optimizēti lasīšanas pieredzei

- **Pamata salikums**: pielāgojieties virsrakstiem, rindkopām, treknrakstam, slīprakstam, pārsvītrojumam, iekļautajam kodam, attēliem un dalīšanas līnijām.
- **Saites animācija**: parādiet pasvītrojuma animāciju, virzot kursoru.
- **Projektu saraksts**: nesakārtotajos sarakstos tiek izmantotas aizzīmes, sakārtotos sarakstos tiek izmantoti noapaļoti fona sērijas numuri, un tie atbalsta vairāku līmeņu ligzdošanu.
- **Uzdevumu saraksts**: nodrošina izvēles rūtiņas stilu, atbalsta uzdevumu un pabeigto statusu.
- **Brīdinājuma padomi**: atbalsta piecu veidu brīdinājuma lodziņus: Piezīme, Padoms, Svarīgi, Brīdinājums un Uzmanību.
- **Tabulas un kods**: nodrošina tabulas rindu izcelšanu, novietojot kursoru, un kodu blokus ar ēnām un gradientu foniem.

## Izmantojiet demonstrāciju

```html
<div class="Md">
  <h1>virsraksts</h1>
  <p>Rindkopas teksts un <a href="#">saite</a></p>
  <hr>
  <h2>Otrā līmeņa tituls</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Uzdevuma vienums</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Piezīmju saturs</p>
  </blockquote>
</div>
```

## stila klase

Stili tiek lietoti HTML elementiem zem konteinera `.Md`.

### Brīdinājuma lodziņa stils

Pievienojiet šādu klases nosaukumu elementam `blockquote` ar `.q` klases nosaukumu, lai pārslēgtu brīdinājuma lodziņu:

- `note`: piezīmes
- `tip`: padoms
- `important`: svarīgi
- `warning`: brīdinājums
- `caution`: Piezīme