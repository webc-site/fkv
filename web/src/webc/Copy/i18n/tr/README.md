# Metni panoya kopyala

Belirtilen öğenin metnini kopyalamak için düğmeye tıklayın.

- Kopyalanacak öğe seçiciyi belirtmek için rel kullanımını destekler
- Belirtilmediği zaman önceki kardeş öğe varsayılan olarak kopyalanır.
- İç İçe Pophover bilgi istemi kutusu
- Uluslararası destek

## Demoyu kullanın

```html
<!-- Belirli öğeleri kopyala -->
<span id="target">İşte kopyalanacak metin</span>
<c-copy rel="#target"></c-copy>

<!-- Önceki öğeler varsayılan olarak kopyalanır -->
<span>önceki öğenin metni</span>
<c-copy></c-copy>
```

## parametre

| mülk | göstermek |
| --- | --- |
| rel | Hedef öğenin seçicisini kopyalayın. İsteğe bağlı |

## durum stili

- `c-pophover.Done`: başarılı kopyalamanın ardından bilgi istemi kutusu stili