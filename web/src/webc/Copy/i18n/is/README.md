# Afritaðu texta á klemmuspjald

Smelltu á hnappinn til að afrita texta tilgreinds þáttar.

- Styður notkun rel til að tilgreina þáttavalið sem á að afrita
- Þegar það er ekki tilgreint er fyrri systkinaþátturinn sjálfgefið afritaður.
- Hreiður Pophover boðkassi
- Alþjóðlegur stuðningur

## Notaðu kynninguna

```html
<!-- Afritaðu tiltekna þætti -->
<span id="target">Hér er textinn sem á að afrita</span>
<c-copy rel="#target"></c-copy>

<!-- Fyrri þættir eru sjálfgefið afritaðir -->
<span>texta fyrri þáttar</span>
<c-copy></c-copy>
```

## færibreytu

| eign | sýna |
| --- | --- |
| rel | Afritaðu veljara markþáttarins. Valfrjálst |

## stöðu stíl

- `c-pophover.Done`: stíll hvetja kassa eftir vel heppnaða afritun