# Səhifə sürüşdürüldükdə avtomatik çökən və ya genişlənən üst naviqasiya paneli

- **Avtomatik Gizlət**: Səhifəni aşağı sürüşdürərkən avtomatik olaraq gizlədin.
- **Avtomatik genişləndirmə**: Səhifə yuxarı sürüşdürüldükdə və ya sürüşdürmə çubuğu yox olduqda avtomatik olaraq göstərilir.
- **Performans Optimizasiyası**: Sürüşməni azaltmaq üçün `requestAnimationFrame` istifadə edin.
- **Resursların buraxılması**: Komponent məhv edildikdə hadisə dinləyiciləri və müşahidəçiləri avtomatik olaraq silinir.

## Demodan istifadə edin

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Link</a>
      </c-nav>
    </nav>
    <div class="content">Aşağı sürüşdürərək naviqasiyanı gizlədin, yuxarı sürüşdürərək göstərin</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## İstifadəyə dair göstərişlər

`<c-vs>`-in uşaq və ya nəsil elementi kimi istifadə edilməlidir.

## stil statusu

Komponent aşağıdakı sinif adını `nav.Nav`-də dəyişdirir:

- `D`: Vəziyyəti göstərin (aşağı və ya yuxarı sürüşdürün).
- `I`: Keçid vəziyyəti gizlədilir.
- `H`: Gizli status.

## stil dəyişənləri

- `--top`: Komponent tərəfindən avtomatik olaraq mənfi hündürlük dəyərinə yenilənir, yerləşdirmə və keçid üçün istifadə olunur.