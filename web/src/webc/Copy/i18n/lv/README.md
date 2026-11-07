# Kopējiet tekstu starpliktuvē

Noklikšķiniet uz pogas, lai kopētu norādītā elementa tekstu.

- Atbalsta rel izmantošanu, lai norādītu kopējamā elementa atlasītāju
- Ja tas nav norādīts, pēc noklusējuma tiek kopēts iepriekšējais brāļa elements.
- Ligzdota Pophover uzvednes lodziņš
- Starptautiskais atbalsts

## Izmantojiet demonstrāciju

```html
<!-- Kopējiet konkrētus elementus -->
<span id="target">Šeit ir teksts, kas jākopē</span>
<c-copy rel="#target"></c-copy>

<!-- Iepriekšējie elementi tiek kopēti pēc noklusējuma -->
<span>iepriekšējā elementa teksts</span>
<c-copy></c-copy>
```

## parametrs

| īpašums | ilustrēt |
| --- | --- |
| rel | Kopējiet mērķa elementa atlasītāju. Pēc izvēles |

## statusa stils

- `c-pophover.Done`: uzvednes lodziņa stils pēc veiksmīgas kopēšanas