# Kopjo tekstin në kujtesën e fragmenteve

Klikoni butonin për të kopjuar tekstin e elementit të specifikuar.

- Mbështet përdorimin e rel për të specifikuar përzgjedhësin e elementit që do të kopjohet
- Kur nuk specifikohet, elementi i mëparshëm i vëllait kopjohet si parazgjedhje.
- Kutia e kërkesës e mbivendosur Pophover
- Mbështetje ndërkombëtare

## Përdorni demonstrimin

```html
<!-- Kopjoni elemente specifike -->
<span id="target">Këtu është teksti që do të kopjohet</span>
<c-copy rel="#target"></c-copy>

<!-- Elementet e mëparshëm kopjohen si parazgjedhje -->
<span>teksti i elementit të mëparshëm</span>
<c-copy></c-copy>
```

## parametri

| prone | ilustrojnë |
| --- | --- |
| rel | Kopjoni përzgjedhësin e elementit të synuar. Fakultative |

## stili i statusit

- `c-pophover.Done`: stili i kutisë së shpejtë pas kopjimit të suksesshëm