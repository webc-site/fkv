# Açılan seçim ilə tab səhifəsi

- Adaptiv eni açılan seçim qutusunun (`<c-select>`) etiketə daxil edilməsini dəstəkləyir, bu, yalnız cari tab səhifəsi aktivləşdirildikdə göstəriləcək.
- Struktur spesifikasiya: Uşaq element etiket naviqasiyası kimi `nav > a` istifadə edir və birbaşa alt element `b` məzmun sahəsi kimi xidmət edir.
- `a[value]` teqi, `a` ilə `c-select` (qrupun dəyər atributuna və seçilmiş seçim dəyərinə əsasən) və `b[slot]` məzmun qovşağını avtomatik əlaqələndirir.
- Hazırda aktivləşdirilmiş elementin `key` atributu vasitəsilə avtomatik olaraq `localStorage` saxlanmasını dəstəkləyir.
- `key + '/' + tab` istifadə edərək, hər bir qrup üçün `localStorage` tarixi alt seçimlərin avtomatik saxlanmasını və bərpasını dəstəkləyir.
- `value` atributu vasitəsilə aktiv tabın göstərilməsini dəstəkləyir.

## Demodan istifadə edin

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>dil</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistemi</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript bir skript dilidir</b>
    <b slot="rs">Rust sistem səviyyəsində proqramlaşdırma dilidir</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS Unix əsaslı qrafik əməliyyat sistemidir</b>
    <b slot="win">Windows, Microsoft tərəfindən hazırlanmış bir əməliyyat sistemidir</b>
  </b>
</c-tab-select>
```

## əmlak

- `key`: localStorage davamlılığı üçün istifadə edilən açar adı
- `value`: hazırda seçilmiş dəyər

## Fərdi tədbirlər

- `change`: Nişanları dəyişdirərkən işə salınır, `e.value` hazırda aktivləşdirilmiş seçimin `value` dəyəridir.