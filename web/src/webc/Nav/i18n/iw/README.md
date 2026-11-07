# סרגל הניווט העליון שמתכווץ או מתרחב באופן אוטומטי כאשר הדף גולש

- **הסתר אוטומטי**: הסתר אוטומטית בעת גלילה מטה בעמוד.
- **הרחבה אוטומטית**: מוצג באופן אוטומטי כאשר הדף גולל למעלה או פס הגלילה נעלם.
- **אופטימיזציה של ביצועים**: השתמש ב-`requestAnimationFrame` כדי להגביל את הגלילה.
- **משאבי שחרור**: מאזינים וצופים לאירועים מוסרים אוטומטית כאשר הרכיב מושמד.

## השתמש בהדגמה

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">לְקַשֵׁר</a>
      </c-nav>
    </nav>
    <div class="content">החלק מטה כדי להסתיר את הניווט, החלק למעלה כדי להציג אותו</div>
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

## הוראות שימוש

חייב לשמש כרכיב ילד או צאצא של `<c-vs>`.

## מצב סגנון

הרכיב מחליף את שם המחלקה הבא ב-`nav.Nav`:

- `D`: הצג סטטוס (החלק למטה או למעלה).
- `I`: מצב המעבר מוסתר.
- `H`: סטטוס מוסתר.

## משתני סגנון

- `--top`: מתעדכן אוטומטית על ידי הרכיב לערך שלילי של גובה, המשמש למיקום ולמעבר.