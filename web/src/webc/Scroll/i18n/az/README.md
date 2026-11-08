# virtual sürüşmə çubuğu

- **İkitərəfli sürüşmə**: Şaquli (`c-vs`) və üfüqi (`c-hs`) sürüşməni təmin edir.
- **Ölçü cavabı**: Məzmun və sahə ölçüsü dəyişikliklərinə nəzarət edin, sürüşdürmə çubuğunun ölçüsünü və mövqeyini avtomatik yeniləyin və daşqın olmadıqda avtomatik gizlədin.
- **İnteraktiv dəstək**: Təkər sürüşdürməsini, sürüşdürməni sürükləməyi və klik yerini təyin etməyi dəstəkləyir.
- **Vəziyyəti Gizlət**: Sürüşdürmə zamanı sürüşdürməni göstərin və genişləndirin və hərəkətsiz olduqda avtomatik gizlədin.

## şaquli sürüşdürmə

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## üfüqi sürüşdürmə

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## üslub hissələri

`::part()` vasitəsilə Shadow DOM daxili elementlərinin üslubunu fərdiləşdirin:

- `::part(scroll)`: Məzmun sahəsini sürüşdürün.
- `::part(bar)`: Sürüşdürmə çubuğu treki.
- `::part(si)`: Sürüşdürmə çubuğu kaydırıcısı.
- `::part(i)`: Daxili sarma qatı.
- `::part(drag)`: Sürüşdürmə zolağındakı trek.

## stil sinfi

- `.drag`: `body` elementinə tətbiq edilib və onun sürüşdürmə vəziyyətində olduğunu göstərir.

## Xüsusi xüsusiyyətlər

- `--si-bg`: Slayder fon rəngi.
- `--si-anim`: Slayder animasiyası.
- `--cursorScrollhSvg`: Üfüqi sürüşdürmə çubuğu hərəkət edərkən siçan kursoru.
- `--cursorScrollvSvg`: Şaquli sürüşdürmə çubuğu hərəkət edərkən siçan kursoru.