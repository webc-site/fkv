# Dəyişdirilə bilən nişanlar

- Seçilmiş nişan vəziyyəti ilə altındakı məzmun sahəsi problemsiz birləşir.
- Struktur spesifikasiyalar: Uşaq elementlər etiket naviqasiyası kimi `nav > a` istifadə etmək üçün sabitlənib və birbaşa alt element `b` məzmun sahəsi kimi istifadə olunur.
- `a[value]` teqini avtomatik olaraq `b[slot]` məzmun qovşağı ilə əlaqələndirin.
- Cari aktiv səhifənin `key` atributu vasitəsilə avtomatik olaraq `localStorage`-ə qədər davam etdirilməsini dəstəkləyir.
- `value` atributu vasitəsilə aktivləşdirilmiş tab səhifəsinin müəyyənləşdirilməsini dəstəkləyir (əgər keş və `value` yoxdursa, defolt olaraq `class="A"` və ya birinci teq olan teq seçiləcək).
- Yüngül Light DOM implementasiyası, Shadow DOM maneəsi yoxdur, stil tənzimləməsi yüksək dərəcədə sərbəstdir.

## Demodan istifadə edin

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">alma</a>
    <a value="banana">banan</a>
    <a value="orange">naringi</a>
  </nav>
  <b slot="apple">Alma qidalandırıcı bir meyvədir</b>
  <b slot="banana">Banan tropik meyvədir</b>
  <b slot="orange">Portağal C vitamini ilə zəngindir</b>
</c-tab>
```

## əmlak

- `key`: localStorage davamlılığı üçün istifadə edilən açar adı
- `value`: hazırda seçilmiş dəyər

## Fərdi tədbirlər

- `change`: Tab dəyişdirildikdə işə salınır, `e.value` hazırda aktivləşdirilmiş teqin `value` dəyəridir.