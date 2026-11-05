# Naviqasiya menyusunu və qayıt düyməsini avtomatik dəyişdirin

- Naviqasiya menyusu yaratmaq və düymə strukturlarını qaytarmaq üçün uşaq elementləri avtomatik sarın
- Avtomatik olaraq marşrutlaşdırma tarixini izləyin və əvvəlki qayıdış yolunu hesablayın
- Marşrut menyu elementinə uyğun gələndə naviqasiya menyusunu göstərin, əks halda geri düyməsini göstərin

## Demodan istifadə edin

```html
<c-nav-l>
  <a href="/">ön səhifə</a>
  <a href="/doc">sənəd</a>
</c-nav-l>
```

## stil sinfi

- `B`: qayıt düyməsinin vəziyyətini aktivləşdirir. Geri düyməsini göstərin və naviqasiya menyusunu gizlədin. Marşrutlaşdırma əsasında komponent daxilində avtomatik keçid.