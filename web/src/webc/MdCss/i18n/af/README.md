# Tipografiestyle geoptimaliseer vir leeservaring

- **Basiese setwerk**: Pas aan by titels, paragrawe, vetdruk, kursief, deurtrek, inlynkode, prente en skeidingslyne.
- **Skakelanimasie**: Wys onderstreep-animasie wanneer jy beweeg.
- **Projeklys**: Ongeordende lyste gebruik kolpunte, geordende lyste gebruik afgeronde agtergrondreeksnommers, en ondersteun multi-vlak nes.
- **Taaklys**: Verskaf merkblokkiestyl, ondersteun taak- en voltooide status.
- **Wenke vir waarskuwing**: Ondersteun vyf soorte waarskuwingsblokkies: Nota, Wenk, Belangrik, Waarskuwing en Aandag.
- **Tabelle en kode**: Voorsien tabelry-sweefverligting, en kodeblokke met skaduwees en gradiënt-agtergronde.

## Gebruik die demo

```html
<div class="Md">
  <h1>titel</h1>
  <p>Paragraafteks en <a href="#">skakel</a></p>
  <hr>
  <h2>Tweede vlak titel</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Taakitem</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Nota-inhoud</p>
  </blockquote>
</div>
```

## styl klas

Style word toegepas op HTML-elemente onder die `.Md`-houer.

### Waarskuwingskasstyl

Voeg die volgende klasnaam by die `blockquote` element met die `.q` klasnaam om die waarskuwingskassie te wissel:

- `note`: Notas
- `tip`: Wenk
- `important`: Belangrik
- `warning`: waarskuwing
- `caution`: Let wel