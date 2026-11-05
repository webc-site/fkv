# Müxtəlif ekran ölçülərinə uyğunlaşdırılmış naviqasiya menyusu

- İş masası: Bağlantılar üfüqi plitələrdə göstərilir
- Mobil versiya: Menyu düyməsini yığışdırın, yan panel siyirtməsini sürüşdürmək üçün klikləyin
- Marşrutun aktivləşdirilməsi: cari yolu və hash dəyərini avtomatik müqayisə edin və uyğun keçidə aktivasiya statusu sinfinin adını əlavə edin `A`
- Avtomatik olaraq bağlan: Mobil terminal bir keçidə və ya kənar panelin xarici fonuna kliklədikdə, yan panel avtomatik olaraq bağlanır.

## istifadə edin

```html
<c-nav-menu>
  <a href="/">ön səhifə</a>
  <a href="/doc">sənəd</a>
  <a href="/about">haqqında</a>
</c-nav-menu>
```

## təsvir etmək

Komponent işə salındıqda, uşaq elementdə `c-side` yoxdursa, mobil menyu düyməsi və `c-side` yan panel avtomatik olaraq yaradılacaq və bütün `a` teqlər yan panelə köçürüləcək.

## stil sinfi

- **A**: Aktivləşdirilmiş keçid avtomatik olaraq sinif adını `A` əlavə edəcək