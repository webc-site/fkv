# تصحيح أخطاء كود HTML وJS عبر الإنترنت

- قم بعرض شريط تصحيح الأخطاء أسفل الكود وقم بتوفير بوابات تصحيح الأخطاء CodePen وJSFiddle
- تذكر نظام التصحيح المحدد تلقائيًا
- يدعم إدخال عمليات رد الاتصال من خلال السمة `.gen` لإنشاء أكواد HTML وCSS وJS ديناميكيًا
- تحديد العناصر الفرعية تلقائيًا باستخدام اسم الفئة `language-js` وتشغيلها في وضع JS

## استخدم العرض التوضيحي

```html
<c-htm-demo><pre><code class="language-html">
<h1>تجريبي</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // العودة [html، css، js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## معلمات الواجهة

### الخصائص والأساليب

- `.gen`: وظيفة رد الاتصال المحقونة. **يجب إدخال رد الاتصال هذا حتى يتم عرض شريط التصحيح. **
  - المعلمة:
    - `type`: نوع المحتوى المُدرج (`HTML = 1`, `JS = 2`).
    - `text`: نص الكود المستخرج من العنصر الفرعي.
  - قيمة الإرجاع: `[htm, css, js]`