# Irakurketa esperientziarako optimizatutako tipografia-estiloak

- **Oinarrizko idazketa**: Egokitu izenburuetara, paragrafoetara, letra lodia, etzana, marratua, lerroko kodea, irudietara eta lerro zatietara.
- **Estekaren animazioa**: bistaratu azpimarratzeko animazioa pasatzean.
- **Proiektuen zerrenda**: ordenatu gabeko zerrendek buletak erabiltzen dituzte, ordenatutako zerrendek atzeko plano biribildutako serie-zenbakiak erabiltzen dituzte eta maila anitzeko habiak onartzen dituzte.
- **Zerrenda Zerrenda**: kontrol-laukiaren estiloa eskaintzen du, egitekoak eta amaitutako egoera onartzen du.
- **Abisu-aholkuak**: bost abisu-koadro mota onartzen ditu: Oharra, Aholkua, Garrantzitsua, Abisua eta Arreta.
- **Taulak eta kodea**: taulako errenkadak pasatzean nabarmentzea eta kode-blokeak itzalak eta gradienteak dituzten atzeko planoak eskaintzen ditu.

## Erabili demoa

```html
<div class="Md">
  <h1>izenburua</h1>
  <p>Paragrafoaren testua eta <a href="#">esteka</a></p>
  <hr>
  <h2>Bigarren mailako titulua</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Zereginaren elementua</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Ohartu edukia</p>
  </blockquote>
</div>
```

## estilo klasea

Estiloak `.Md` edukiontziaren azpian dauden HTML elementuei aplikatzen zaizkie.

### Alerta-kutxa estiloa

Gehitu klase-izen hau `blockquote` elementuan `.q` klase-izenarekin alerta-koadroa aldatzeko:

- `note`: Oharrak
- `tip`: Aholkua
- `important`: Garrantzitsua
- `warning`: abisua
- `caution`: Oharra