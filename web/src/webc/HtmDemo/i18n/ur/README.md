# HTML اور JS کوڈ کو آن لائن ڈیبگ کرنا

- کوڈ کے نیچے ڈیبگنگ بار رینڈر کریں اور CodePen اور JSFiddle ڈیبگنگ پورٹل فراہم کریں
- منتخب کردہ ڈیبگنگ پلیٹ فارم کو خود بخود یاد رکھیں
- متحرک طور پر HTML، CSS اور JS کوڈ بنانے کے لیے `.gen` انتساب کے ذریعے کال بیکس کو انجیکشن کرنے کی حمایت کرتا ہے
- `language-js` کلاس کے نام سے بچوں کے عناصر کی خود بخود شناخت کریں اور JS موڈ میں چلائیں۔

## ڈیمو استعمال کریں۔

```html
<c-htm-demo><pre><code class="language-html">
<h1>ڈیمو</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // واپسی [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## انٹرفیس کے پیرامیٹرز

### خصوصیات اور طریقے

- `.gen`: انجکشن شدہ کال بیک فنکشن۔ **ڈیبگ بار کو ظاہر کرنے کے لیے یہ کال بیک انجیکشن ہونا چاہیے۔ **
  - پیرامیٹر:
    - `type`: داخل کردہ مواد کی قسم (`HTML = 1`، `JS = 2`)۔
    - `text`: کوڈ کا متن چائلڈ عنصر سے نکالا گیا ہے۔
  - واپسی کی قدر: `[htm, css, js]`