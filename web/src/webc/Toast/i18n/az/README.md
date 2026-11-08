# Səhifənin kənarında açılan pəncərə

- **Funksiya çağırışı**: Normal və xəta tələb edən interfeysləri təmin edir
- **Avtomatik yığın**: Birdən çox məsləhət avtomatik olaraq məsafəni və şaquli yığını hesablayır
- **Planlı və əl ilə söndürmə**: Planlaşdırılmış və əl ilə söndürməni dəstəkləyir

## Demodan istifadə edin

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "İcazəli məzmun";
});

toastErr((el) => {
  el.textContent = "Xəta məzmunu";
});

toast(
  (el) => {
    el.textContent = "99 saniyədən sonra avtomatik bağlan";
  },
  99
);
```

## İnterfeys parametrləri

### toast(render, timeout)

Tələblər yaradın və göstərin.

- `render`: `(el) => void`, geri çağırış göstərilir, `el` sorğu qutusu elementidir.
- `timeout`: `Number`, fasilə saniyələri. Defolt 9. Avtomatik olaraq bağlanmamaq üçün 0-a təyin edin.
- Qaytarma dəyəri: sorğu qutusu elementi. Dəstəkləyir `el.close()` deaktiv.

### toastErr(render, timeout)

Səhv mesajları yaradın və göstərin. Parametrlər `toast` stil sinfi ilə eynidir.

## stil sinfi

- `.Toast`: Məlumat qutusu əsas sinfi.
- `.ERR`: Xəta statusu.
- `.animated` / `.fadeInR` / `.fadeOutR`: animasiyalı keçid.
- `.x`: Bağlama düyməsi.