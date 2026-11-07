# Yan tərəfdən sürüşən çekmece panelləri

- Avtomatik olaraq daxili sürüşmə strukturları yaradın
- Avtomatik olaraq çökdürmək üçün xarici sahəyə klikləyin
- Bağlamaq üçün sola və sağa sürüşdürün
- Sol və sağ istiqamətlərdə yan sürüşmə yerləşdirmə
- Açıq və qapalı tədbirləri təmin edin

## Demodan istifadə edin

```html
<button class="open-btn">Açıq</button>

<c-side class="right">
  <div>Çekmece məzmunu</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interfeys

### üsul

- `open(el)`: İxrac funksiyası, siyirməni açın
- `close(el)`: İxrac funksiyası, çekmeceyi bağlayın
- `el.open()`: nümunə metodu, açıq siyirtmə
- `el.close()`: nümunə metodu, çekmeceyi bağlayın

### hadisə

- `open`: Aktiv olduqda yanır
- `close`: Bağlandıqda yanır

## stil sinfi

- `.right`: yerləşdirməni sağa sürüşdürün
- `.top`: Yerləşdirməni sürüşdürün
- `.bottom`: yerləşdirmədən sürüşdürün
- (Qurulmayıbsa, defolt olaraq sola sürüşdürün)
- `.open`: açıq status
- `.H`: gizli vəziyyət
- `.T`: animasiya keçid vəziyyəti