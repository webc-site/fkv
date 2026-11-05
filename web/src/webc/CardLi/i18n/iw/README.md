# טען והצג את רשימת הכרטיסים באופן אסינכרוני

- **טעינה אסינכרונית**: קבל נתונים באמצעות פונקציה אסינכרונית
- **תצוגת סטטוס**: תומך בשלושה מצבים: טעינה, נתונים ריקים וכרטיס עיבוד.
- **פריסה גמישה**: הקלפים מסודרים בשורות

## השתמש בהדגמה

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["כותרת " + i, "תיאור " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## פרמטרים של ממשק

### CardLi(loader)

- **טוען**: `() => Promise<Array<[string, string, string]>>`, פונקציית טעינת נתונים אסינכרונית. הפורמט של כל מפתח משנה הוא `[שם, מבוא, קישור]`.

## שיעור סגנון

- `.CardLi`: אלמנט חיצוני
- `.Card`: רכיב כרטיס
- `.Ing`: טוען אלמנט מונפש