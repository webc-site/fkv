# قم بإنشاء وعرض طبقة منبثقة باستخدام زر الإغلاق

- استدعاء وظيفي لإنشاء وعرض الطبقة المنبثقة
- زر إغلاق مدمج، انقر لإغلاق العنصر وإزالته من DOM
- اضغط على Esc لإلغاء التركيز على مربع الإدخال أو إغلاق الطبقة المنبثقة

## استخدم العرض التوضيحي

```html
<button>انقر لتنبثق</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "text";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## معلمات الواجهة

تقوم وظيفة التصدير الافتراضية بإرجاع `HTMLDialogElement`، وتمت إضافة فئات النمط `Box` و `X` إلى عنصر الحوار.

## فئة النمط

### `.Box.X`

يتم تطبيقه على العنصر `dialog`، ويحدد نمط الحوار.

### `a.X`

يتم تطبيقه على العنصر `a`، ويحدد نمط زر الإغلاق.