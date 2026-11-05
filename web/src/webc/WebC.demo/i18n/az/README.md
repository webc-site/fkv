# Komponent demolarını, sənədləri və quraşdırma təlimatlarını göstərin və göstərin

- README sənədinin başlığını, girişini və məzmununu çıxarın və təhlil edin
- Komponentlərin interaktiv demolarını işə salmaq və üslubları təcrid etmək üçün Sandbox
- Sənədlərdəki kod bloklarını interaktiv təqdimatlara çevirin
- Adaptiv tərtibat, geniş ekranlı sütun ekranı, dar ekran nişanı keçid ekranı

## Demodan istifadə edin

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Əməliyyat interfeysini işə salın və əldə edin
const [setMod, setMd, unMount] = WebCDemo(el);
```

## İnterfeys parametrləri

### `WebCDemo(el)`

Komponenti işə salın.

- **parametr**
  - `el`:`HTMLElement`, komponent konteyneri.
- **Qayıdış dəyəri**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Demo modulu quraşdırın.
  - `setMd(name, md, lang)`: Komponent adını, README sənəd məzmununu və cari dili təyin edin.
  - `unMount()`: Komponenti boşaltın və hadisə dinləyicisini təmizləyin.

### Demo modul `mod` atributu

- `CSS`:`string`, komponentin üslub cədvəli yolu.
- `HTM`:`string`, komponentin HTML şablonu.
- `default`:`function(root)`, komponentin başlanğıc funksiyası, kölgə kök konteynerini qəbul edir.

## stil sinfi

### `.M`

məzmun sahəsi.

### `.L`

Sol paneldə təlimatlar və sənədlər var.

### `.R`

Demodan ibarət sağ panel.

### `c-nav`

Dar ekran naviqasiya paneli.