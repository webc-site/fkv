# مزيج من مربع الاختيار المنسدل والزر

- قم بلصق مربع التحديد والزر أفقيًا
- حفظ الحالة المحددة تلقائيًا في localStorage
- اعتراض نقرات الزر وإرسال الأحداث المخصصة

## استخدم العرض التوضيحي

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">الخيار 1</option>
    <option value="option2">الخيار 2</option>
  </c-select>
  <button>يجري</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ملكية

- `key`: اسم المفتاح المستخدم لاستمرارية التخزين المحلي
- `value`: القيمة المحددة حاليًا

## حدث

- `submit`: يتم إرساله عند النقر فوق الزر، `e.value` يحمل القيمة المحددة حاليًا