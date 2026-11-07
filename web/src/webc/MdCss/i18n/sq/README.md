# Stilet e tipografisë të optimizuara për përvojën e leximit

- **Vendosja bazë**: Përshtatuni titujve, paragrafëve, shkronjave të theksuara, kursive, gërmimeve, kodeve në linjë, figurave dhe vijave ndarëse.
- **Animacioni i lidhjes**: Shfaq animacionin e nënvizuar kur rri pezull.
- **Lista e projekteve**: Listat e pa renditura përdorin pika, listat e renditura përdorin numra serialë të rrumbullakosura në sfond dhe mbështesin folenë me shumë nivele.
- **Lista e detyrave**: Ofron stilin e kutisë së kontrollit, mbështet statusin e detyrave dhe të përfunduara.
- **Këshilla paralajmëruese**: Mbështet pesë lloje të kutive paralajmëruese: Shënim, Këshillë, E rëndësishme, Paralajmërim dhe Kujdes.
- **Tabelat dhe kodi**: Ofron theksim të varur nga rreshti i tabelës dhe blloqe kodesh me hije dhe sfonde gradient.

## Përdorni demonstrimin

```html
<div class="Md">
  <h1>titull</h1>
  <p>Teksti i paragrafit dhe <a href="#">lidhja</a></p>
  <hr>
  <h2>Titulli i nivelit të dytë</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Artikulli i detyrës</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Përmbajtja e shënimeve</p>
  </blockquote>
</div>
```

## klasë stili

Stilet aplikohen në elementët HTML nën kontejnerin `.Md`.

### Stili i kutisë së alarmit

Shtoni emrin e klasës vijuese në elementin `blockquote` me emrin e klasës `.q` për të ndryshuar kutinë e sinjalizimit:

- `note`: Shënime
- `tip`: Këshillë
- `important`: E rëndësishme
- `warning`: paralajmërim
- `caution`: Shënim