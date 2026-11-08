# Mətni panoya kopyalayın

Göstərilən elementin mətnini kopyalamaq üçün düyməni basın.

- Kopyalanacaq element seçicisini təyin etmək üçün rel istifadəsini dəstəkləyir
- Müəyyən edilmədikdə, əvvəlki qardaş element standart olaraq kopyalanır.
- İç-içə Pophover sorğu qutusu
- Beynəlxalq dəstək

## Demodan istifadə edin

```html
<!-- Xüsusi elementləri kopyalayın -->
<span id="target">Budur kopyalanacaq mətn</span>
<c-copy rel="#target"></c-copy>

<!-- Əvvəlki elementlər standart olaraq kopyalanır -->
<span>əvvəlki elementin mətni</span>
<c-copy></c-copy>
```

## parametr

| əmlak | təsvir etmək |
| --- | --- |
| rel | Hədəf elementin seçicisini kopyalayın. Könüllü |

## status üslubu

- `c-pophover.Done`: uğurlu surətdən sonra sorğu qutusu üslubu