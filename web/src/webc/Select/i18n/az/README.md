# Açılan seçim, eni cari elementə uyğunlaşır

Doğma `select` eni standart olaraq ən uzun seçim mətni ilə müəyyən edilir. Qısa seçimləri dəyişdirərkən, görünüşə təsir edən çox ağ boşluq var.

Bu komponent şəffaf şəkildə `b` teqinin üstündəki yerli `select` elementi əhatə edir və genişliyin hazırda seçilmiş mətnin uzunluğuna dəqiq uyğunlaşmasına imkan verir.

- Bütün terminallarda yerli `select` açılan menyu üslublarını və əlçatanlıq dəstəyini qoruyun
- Tam klaviatura Tab naviqasiyası və istiqamət düymələrinin dəyişdirilməsi kimi yerli interaktiv əməliyyatları dəstəkləyir.
- Həddindən artıq boş boşluğu aradan qaldırmaq üçün genişliyi cari seçimə uyğunlaşdırın

## Demodan istifadə edin

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```