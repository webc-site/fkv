# צור והצג שכבה קופצת עם כפתור סגירה

- קריאה פונקציונלית ליצור ולהציג שכבת קופץ
- לחצן סגירה מובנה, לחץ כדי לסגור ולהסיר את האלמנט מה-DOM
- הקש Esc כדי לבטל את המיקוד של תיבת הקלט או לסגור את השכבה הקופצת

## השתמש בהדגמה

```html
<button>לחץ כדי לצוץ</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "טקסט";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## פרמטרים של ממשק

פונקציית הייצוא המוגדרת כברירת מחדל מחזירה `HTMLDialogElement`, ומחלקות הסגנון `Box` ו`X` נוספו לרכיב הדו-שיח.

## שיעור סגנון

### `.Box.X`

מוחל על האלמנט `dialog`, מציין את סגנון הדו-שיח.

### `a.X`

מוחל על האלמנט `a`, מציין את סגנון לחצן הסגירה.