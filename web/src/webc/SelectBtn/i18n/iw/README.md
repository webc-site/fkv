# שילוב של תיבת בחירה נפתחת וכפתור

- חברו את תיבת הבחירה והלחצן אופקית
- שמור באופן אוטומטי את המצב שנבחר ב-localStorage
- ייירט לחיצות על כפתור ושליחה של אירועים מותאמים אישית

## השתמש בהדגמה

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">אפשרות 1</option>
    <option value="option2">אפשרות 2</option>
  </c-select>
  <button>לָרוּץ</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## נֶכֶס

- `key`: שם המפתח המשמש להתמדה של LocalStorage
- `value`: הערך הנבחר כעת

## מִקרֶה

- `submit`: נשלח בעת לחיצה על הכפתור, `e.value` נושא את הערך הנבחר כעת