# Szöveg másolása a vágólapra

Kattintson a gombra a megadott elem szövegének másolásához.

- Támogatja a rel használatát a másolandó elemválasztó megadásához
- Ha nincs megadva, a rendszer alapértelmezés szerint az előző testvér elemet másolja.
- Beágyazott Pophover prompt doboz
- Nemzetközi támogatás

## Használd a demót

```html
<!-- Adott elemek másolása -->
<span id="target">Itt van a másolandó szöveg</span>
<c-copy rel="#target"></c-copy>

<!-- Az előző elemek alapértelmezés szerint másolásra kerülnek -->
<span>az előző elem szövege</span>
<c-copy></c-copy>
```

## paraméter

| ingatlan | szemléltet |
| --- | --- |
| rel | Másolja a célelem választóját. Választható |

## állapotstílus

- `c-pophover.Done`: prompt box stílus sikeres másolás után