# صفحة علامة التبويب مع تحديد القائمة المنسدلة

- يدعم تضمين مربع التحديد المنسدل للعرض التكيفي (`<c-select>`) في التسمية، والذي سيتم عرضه فقط عند تنشيط صفحة علامة التبويب الحالية.
- المواصفات الهيكلية: يستخدم العنصر الفرعي `nav > a` كمنطقة للتنقل، ويعمل العنصر الفرعي المباشر `b` كمنطقة محتوى.
- يتم تلقائيًا ربط العلامة `a[value]`، `a` مع `c-select` (استنادًا إلى سمة قيمة المجموعة وقيمة الخيار المحدد) وعقدة المحتوى `b[slot]`.
- يدعم استمرار العنصر المنشط حاليًا تلقائيًا إلى `localStorage` من خلال السمة `key`.
- يدعم الحفظ التلقائي واستعادة التحديدات الفرعية التاريخية لكل مجموعة إلى `localStorage` باستخدام `key + '/' + tab`.
- يدعم تحديد علامة التبويب النشطة من خلال السمة `value`.

## استخدم العرض التوضيحي

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>لغة</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>نظام</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">جافا سكريبت هي لغة البرمجة النصية</b>
    <b slot="rs">Rust هي لغة برمجة على مستوى الأنظمة</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS هو نظام تشغيل رسومي يستند إلى Unix</b>
    <b slot="win">Windows هو نظام تشغيل تم تطويره بواسطة Microsoft</b>
  </b>
</c-tab-select>
```

## ملكية

- `key`: اسم المفتاح المستخدم لاستمرارية التخزين المحلي
- `value`: القيمة المحددة حاليًا

## الأحداث المخصصة

- `change`: يتم تشغيله عند التبديل بين علامات التبويب، `e.value` هي قيمة `value` للخيار المنشط حاليًا.