# דף כרטיסייה עם בחירה נפתחת

- תומך בהטמעת תיבת בחירה נפתחת של רוחב אדפטיבי (`<c-select>`) בתווית, שתוצג רק כאשר דף הכרטיסייה הנוכחי יופעל.
- מפרט מבני: רכיב הצאצא משתמש ב`nav > a` בתור ניווט בתווית, ורכיב הצאצא הישיר `b` משמש כאזור התוכן.
- משייך אוטומטית את התג `a[value]`, `a` ל`c-select` (בהתבסס על תכונת הערך של הקבוצה וערך האופציה שנבחר) ולצומת התוכן `b[slot]`.
- תומך בשמירה אוטומטית של הפריט המופעל כעת ל`localStorage` דרך התכונה `key`.
- תומך בשמירה ושחזור אוטומטיים של תתי-בחירות היסטוריות עבור כל קבוצה ל`localStorage` באמצעות `key + '/' + tab`.
- תומך בציון הכרטיסייה הפעילה דרך התכונה `value`.

## השתמש בהדגמה

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>שָׂפָה</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>מַעֲרֶכֶת</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript היא שפת סקריפטים</b>
    <b slot="rs">Rust היא שפת תכנות ברמת המערכת</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS היא מערכת הפעלה גרפית מבוססת יוניקס</b>
    <b slot="win">Windows היא מערכת הפעלה שפותחה על ידי מיקרוסופט</b>
  </b>
</c-tab-select>
```

## נֶכֶס

- `key`: שם המפתח המשמש להתמדה של LocalStorage
- `value`: הערך הנבחר כעת

## אירועים מותאמים אישית

- `change`: מופעל בעת החלפת כרטיסיות, `e.value` הוא הערך `value` של האפשרות המופעלת כעת.