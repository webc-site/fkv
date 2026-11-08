# إنشاء وعرض الطبقة المنبثقة

- استدعاء وظيفي لإنشاء وعرض الطبقة المنبثقة
- إزالة العناصر تلقائيًا من DOM عند إغلاقها
- منع سلوك إلغاء مفتاح Esc

## استخدم العرض التوضيحي

```html
<button>انقر لتنبثق</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "العنوان";
  description.textContent = "النص"؛
  button.className = "Btn Main";
  Button.textContent = "موافق"؛
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## معلمات الواجهة

### الوظائف المصدرة الافتراضية

`Box()`

- **قيمة الإرجاع**: `HTMLDialogElement`، تمت إضافة عنصر الحوار لفئة النمط `Box`.

## فئة النمط

### `.Box`

ينطبق على العنصر `dialog` لتعيين الموضع والتوسيط وتمويه قناع الخلفية.

### `.Lg`

فئة نمط يتم تطبيقها على المحتوى الداخلي، وتعيين لون الخلفية، والزوايا الدائرية، والظل والتخطيط الرأسي، وتحديد تخطيط `h3` و `p` الداخلي.