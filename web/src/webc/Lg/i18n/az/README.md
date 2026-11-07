# Buzlu şüşə fon və kənar vurğu effekti

Yarı şəffaf haşiyə və daxili kölgə mikro relyef kənar vurğuları yaradır.
Düymə və giriş qutusu komponentlərinin əsas üslub asılılığı kimi.

## Demodan istifadə edin

```html
<main class="Lg">
  <article>
    <h3>başlıq</h3>
    <pre>Məzmun mətni</pre>
  </article>
</main>
```

## stil sinfi

### `.Lg`
Kartın əsas üslubu. `backdrop-filter` tutqunluq, `border` mikro relyefli haşiyə və `box-shadow` daxili kölgə tətbiq edin. Hoverdə `svg/glass.svg#h` filtrini tətbiq edin.

### `main.Lg`
Səhifənin əsas blok konteyneri. Həssas doldurma ilə yuvarlaqlaşdırılmış künclər `24px` (`16px` eni `800px`-dən azdırsa, `24px` əks halda).

### `.Btn`, `.BtnC`, `.Input`
Əsas komponent sinfinin adı. Bu üslubu təqdim etdikdən sonra siz avtomatik olaraq çevik layout, iyerarxik əlaqələr və buzlu şüşənin əsas xüsusiyyətlərini əldə edəcəksiniz.