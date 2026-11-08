# Copiar texto al portapapeles

Haga clic en el botón para copiar el texto del elemento especificado.

- Admite el uso de rel para especificar el selector de elementos que se copiará
- Cuando no se especifica, el elemento hermano anterior se copia de forma predeterminada.
- Cuadro de aviso Pophover anidado
- Apoyo internacional

## Usa la demostración

```html
<!-- Copiar elementos específicos -->
<span id="target">Aquí está el texto a copiar.</span>
<c-copy rel="#target"></c-copy>

<!-- Los elementos anteriores se copian de forma predeterminada. -->
<span>el texto del elemento anterior</span>
<c-copy></c-copy>
```

## parámetro

| propiedad | ilustrar |
| --- | --- |
| rel | Copie el selector del elemento de destino. Opcional |

## estilo de estado

- `c-pophover.Done`: estilo del cuadro de aviso después de una copia exitosa