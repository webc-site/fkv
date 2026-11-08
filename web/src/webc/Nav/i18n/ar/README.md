# شريط التنقل العلوي الذي يتم طيه أو توسيعه تلقائيًا عند تمرير الصفحة

- **إخفاء تلقائي**: إخفاء تلقائي عند التمرير لأسفل الصفحة.
- **التوسيع التلقائي**: يتم عرضه تلقائيًا عند تمرير الصفحة لأعلى أو عند اختفاء شريط التمرير.
- **تحسين الأداء**: استخدم `requestAnimationFrame` لتقييد التمرير.
- **تحرير الموارد**: تتم إزالة مستمعي الحدث ومراقبيه تلقائيًا عند تدمير المكون.

## استخدم العرض التوضيحي

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">وصلة</a>
      </c-nav>
    </nav>
    <div class="content">اسحب لأسفل لإخفاء التنقل، ثم اسحب لأعلى لإظهاره</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## تعليمات الاستخدام

يجب استخدامه كعنصر فرعي أو عنصر سليل لـ `<c-vs>`.

## حالة النمط

يقوم المكون بتبديل اسم الفئة التالي على `nav.Nav`:

- `D`: إظهار الحالة (مرر لأسفل أو في الأعلى).
- `I`: يتم إخفاء الحالة الانتقالية.
- `H`: الحالة المخفية.

## متغيرات النمط

- `--top`: يتم تحديثه تلقائيًا بواسطة المكون إلى قيمة سالبة للارتفاع، وتستخدم لتحديد المواقع والانتقال.