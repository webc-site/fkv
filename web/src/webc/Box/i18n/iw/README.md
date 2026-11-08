# צור והצג שכבת קופץ

- קריאה פונקציונלית ליצור ולהציג שכבת קופץ
- הסר אוטומטית אלמנטים מה-DOM כאשר הם סגורים
- מנע התנהגות של ביטול מפתח Esc

## השתמש בהדגמה

```html
<button>לחץ כדי לצוץ</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "כותרת";
  description.textContent = "טקסט";
  button.className = "Btn Main";
  button.textContent = "אישור";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## פרמטרים של ממשק

### פונקציות מיוצאות ברירת מחדל

`Box()`

- **ערך החזרה**: `HTMLDialogElement`, רכיב הדו-שיח של מחלקת הסגנון `Box` נוסף.

## שיעור סגנון

### `.Box`

חל על האלמנט `dialog` כדי להגדיר מיקום, מרכוז וטשטוש מסיכת רקע.

### `.Lg`

מחלקת סגנון המוחלת על התוכן הפנימי, קובעת את צבע הרקע, הפינות המעוגלות, צל ופריסה אנכית, והגדרת הפריסה של ה`h3` ו`p` הפנימיות.