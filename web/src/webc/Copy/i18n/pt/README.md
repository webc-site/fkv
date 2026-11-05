# Copiar texto para a área de transferência

Clique no botão para copiar o texto do elemento especificado.

- Suporta o uso de rel para especificar o seletor de elemento a ser copiado
- Quando não especificado, o elemento irmão anterior é copiado por padrão.
- Caixa de prompt Pophover aninhada
- Apoio internacional

## Use a demonstração

```html
<!-- Copie elementos específicos -->
<span id="target">Aqui está o texto a ser copiado</span>
<c-copy rel="#target"></c-copy>

<!-- Os elementos anteriores são copiados por padrão -->
<span>o texto do elemento anterior</span>
<c-copy></c-copy>
```

## parâmetro

| propriedade | ilustrar |
| --- | --- |
| rel | Copie o seletor do elemento de destino. Opcional |

## estilo de status

- `c-pophover.Done`: estilo da caixa de prompt após cópia bem-sucedida