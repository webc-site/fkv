# Բացվող ընտրության տուփի և կոճակի համադրություն

- Հորիզոնականորեն միացրեք ընտրության տուփը և կոճակը
- Ավտոմատ կերպով պահպանեք ընտրված վիճակը localStorage-ում
- Ընդհատել կոճակի սեղմումները և ուղարկել հատուկ իրադարձություններ

## Օգտագործեք ցուցադրությունը

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Տարբերակ 1</option>
    <option value="option2">Տարբերակ 2</option>
  </c-select>
  <button>վազել</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## սեփականություն

- `key`. բանալի անուն, որն օգտագործվում է localStorage-ի կայունության համար
- `value`՝ ներկայումս ընտրված արժեք

## իրադարձություն

- `submit`. ուղարկվում է, երբ կոճակը սեղմվում է, `e.value` կրում է տվյալ պահին ընտրված արժեքը