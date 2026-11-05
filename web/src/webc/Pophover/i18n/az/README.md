# üzən bubble sorğusu

Siçan üzərinə qoyulduqda və ya element aktiv olduqda hover alət ipucu məlumatını göstərin.

- Doğma Veb Komponentinə əsaslanır, yüngül və yüksək performanslıdır
- Dörd istiqamətdə adaptiv məkan yerləşdirməni dəstəkləyir (class="yuxarı/aşağı/sol/sağ")
- O, qara şaxtalı şüşə faktura qradiyenti fonundan və incə fiziki kənar kontur vurğuları ilə ağ simvollardan istifadə edir.
- Daxili mövqeyə uyğunlaşma hesablaması, görünüş pəncərəsində qalan boş yerə görə pop-up istiqamətini avtomatik tənzimləyir
- Üzən bildiriş məzmununu göstərmək üçün `slot="pop"` istifadə edin

## Demodan istifadə edin

```html
<!-- İstiqaməti avtomatik hesablayın (tövsiyə olunur, sinif elan etməyə ehtiyac yoxdur) -->
<c-pophover>
  <button>avtomatik</button>
  <div slot="pop">Bu baloncuk ucundur</div>
</c-pophover>

<!-- sabit istiqamət -->
<c-pophover class="left">
  <button>Sol</button>
  <div slot="pop">Bu baloncuk ucundur</div>
</c-pophover>

<c-pophover class="right">
  <button>sağ</button>
  <div slot="pop">Bu baloncuk ucundur</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Aşağı</button>
  <div slot="pop">Bu baloncuk ucundur</div>
</c-pophover>

<c-pophover class="top">
  <button>üstün</button>
  <div slot="pop">Bu baloncuk ucundur</div>
</c-pophover>
```

## CSS dəyişənləri

Fərdi üslublar aşağıdakı CSS dəyişənləri vasitəsilə dəstəklənir:

| dəyişən adı | standart dəyər | təsvir etmək |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Bubble alət ipucu və tetikleyici element arasındakı boşluq |
| `--pophover-margin` | `24px` | Adaptiv yerləşdirmə hesablanması zamanı Viewport kənar təhlükəsizlik marjası |
| `--pophover-bg-top` | `#18181ce0` | Bubble gradient fon üst rəngi |
| `--pophover-bg-mid` | `#121215e5` | Baloncuğun sol və sağ tərəflərindəki oxların fon rəngi |
| `--pophover-bg-bottom` | `#0c0c0feb` | Bubble qradiyenti fonunun aşağı və yuxarı və aşağı yan oxlarının rəngi |

## İşarə üçün göstəriş üslubu (`.Ico`)

Saf ikona tipli üzən əmr düymələri üçün (məsələn, "Redaktə et", "Sil" və s.) xarici konteynerə və ya komponentin özünə `.Ico` sinif adını əlavə etməklə vahid ikona və hover üslubunu tətbiq edə bilərsiniz.

### Struktur nümunəsi

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">redaktə et</div>
  </c-pophover>
</b>
```

Əsas tərtibat, ölçü (18x18px) və hover rəng dəyişiklikləri `.Ico > c-pophover > a`-də birləşdirilib. Tərtibatçılar yalnız müvafiq ikon faylını ayrıca göstərməlidirlər, məsələn:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```