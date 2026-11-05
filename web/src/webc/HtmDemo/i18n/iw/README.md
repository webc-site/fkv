# איתור באגים בקוד HTML ו-JS באינטרנט

- צור סרגל איתור באגים מתחת לקוד וספק פורטלים לאיתור באגים CodePen ו-JSFiddle
- זכור אוטומטית את פלטפורמת ניפוי הבאגים שנבחרה
- תומך בהזרקת התקשרויות חוזרות דרך התכונה `.gen` ליצירת קוד HTML, CSS ו-JS באופן דינמי
- זהה אוטומטית רכיבי צאצא עם `language-js` שם מחלקה והפעל במצב JS

## השתמש בהדגמה

```html
<c-htm-demo><pre><code class="language-html">
<h1>הַדגָמָה</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // החזר [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## פרמטרים של ממשק

### מאפיינים ושיטות

- `.gen`: פונקציית התקשרות חוזרת מוזרקת. **התקשרות חוזרת זו חייבת להיות מוזרקת כדי שסרגל ניפוי הבאגים יוצג. **
  - פָּרָמֶטֶר:
    - `type`: סוג תוכן מוזרק (`HTML = 1`, `JS = 2`).
    - `text`: טקסט הקוד שחולץ מהאלמנט הבן.
  - ערך החזרה: `[htm, css, js]`