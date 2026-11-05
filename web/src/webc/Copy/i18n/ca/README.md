# Copia el text al porta-retalls

Feu clic al botó per copiar el text de l'element especificat.

- Admet l'ús de rel per especificar el selector d'elements a copiar
- Quan no s'especifica, l'element germà anterior es copia per defecte.
- Quadre d'indicació de Pophover niu
- Suport internacional

## Utilitzeu la demostració

```html
<!-- Copia elements específics -->
<span id="target">Aquí teniu el text a copiar</span>
<c-copy rel="#target"></c-copy>

<!-- Els elements anteriors es copien per defecte -->
<span>el text de l'element anterior</span>
<c-copy></c-copy>
```

## paràmetre

| propietat | il·lustrar |
| --- | --- |
| rel | Copieu el selector de l'element objectiu. Opcional |

## estil d'estat

- `c-pophover.Done`: estil de quadre de sol·licitud després de la còpia correcta