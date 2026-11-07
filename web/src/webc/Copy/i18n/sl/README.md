# Kopiraj besedilo v odložišče

Kliknite gumb za kopiranje besedila določenega elementa.

- Podpira uporabo rel za določanje izbirnika elementa, ki naj se kopira
- Če ni določen, se prejšnji sorodni element kopira privzeto.
- Ugnezdeno pozivno polje Pophover
- Mednarodna podpora

## Uporabite demo

```html
<!-- Kopirajte določene elemente -->
<span id="target">Tukaj je besedilo, ki ga želite kopirati</span>
<c-copy rel="#target"></c-copy>

<!-- Predhodni elementi so privzeto kopirani -->
<span>besedilo prejšnjega elementa</span>
<c-copy></c-copy>
```

## parameter

| premoženje | ilustrirati |
| --- | --- |
| rel | Kopirajte izbirnik ciljnega elementa. Neobvezno |

## statusni slog

- `c-pophover.Done`: slog polja za poziv po uspešnem kopiranju