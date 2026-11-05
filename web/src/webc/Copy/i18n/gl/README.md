# Copiar texto no portapapeis

Fai clic no botón para copiar o texto do elemento especificado.

- Admite o uso de rel para especificar o selector de elementos que se vai copiar
- Cando non se especifica, o elemento irmán anterior cópiase por defecto.
- Caixa de indicadores de pophover anidado
- Apoio internacional

## Use a demostración

```html
<!-- Copiar elementos específicos -->
<span id="target">Aquí tedes o texto a copiar</span>
<c-copy rel="#target"></c-copy>

<!-- Os elementos anteriores cópianse por defecto -->
<span>o texto do elemento anterior</span>
<c-copy></c-copy>
```

## parámetro

| propiedade | ilustrar |
| --- | --- |
| rel | Copia o selector do elemento de destino. Opcional |

## estilo de estado

- `c-pophover.Done`: estilo de caixa de aviso tras a copia correcta