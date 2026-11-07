# Iekraušanas vai apstrādes statusa indikators

- Apvienojiet kursoru gaidīšanas stāvoklī
- Rāda pulsācijas animāciju, lai norādītu apstrādes statusu
- Lietojiet veidlapai aizmiglošanas masku, lai novērstu dublikātus

## Izmantojiet demonstrāciju

```html
<!-- neatkarīgs iekraušanas indikators -->
<div class="Ing"></div>

<!-- Veidlapas ielāde ar masku -->
<form class="Ing">
  <input type="text" placeholder="Ievades lodziņš">
  <button type="submit">iesniegt</button>
</form>
```

## stila klase

### `.Ing`
- Rādītāja statuss ir `wait`.
- Pseidoelements `::before` atveido pulsācijas animāciju.
- Lietojot elementā `form`, pseidoelements `::after` atveido 1 pikseļu aizmiglošanas masku, atspējojot mijiedarbību un teksta atlasi.