# Yükləmə və ya emal statusunun göstəricisi

- Kursoru gözləmə vəziyyətinə birləşdirin
- Emal vəziyyətini göstərmək üçün dalğalanma animasiyasını göstərir
- Dublikat təqdimlərin qarşısını almaq üçün formaya tutqunluq maskası tətbiq edin

## Demodan istifadə edin

```html
<!-- müstəqil yükləmə göstəricisi -->
<div class="Ing"></div>

<!-- Forma maska ​​ilə yüklənir -->
<form class="Ing">
  <input type="text" placeholder="Giriş qutusu">
  <button type="submit">təqdim edin</button>
</form>
```

## stil sinfi

### `.Ing`
- Göstərici statusu `wait`dir.
- Pseudo element `::before` dalğalanma animasiyasını göstərir.
- `form` elementində istifadə edildikdə, psevdo-element `::after` qarşılıqlı əlaqəni və mətn seçimini deaktiv edərək 1px bulanıqlıq maskası təqdim edir.